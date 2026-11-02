"""Scratch-only accounting tests. Never read the real state database."""
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import time
import unittest
from datetime import datetime, timedelta
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('ai_local', Path(__file__).parents[1] / 'ai-local-usage.py')
assert SPEC is not None and SPEC.loader is not None
usage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(usage)


class LocalUsageTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR'))
        self.home = Path(self.tmp.name)
        self.conn = sqlite3.connect(self.home / 'state.db')
        self.conn.row_factory = sqlite3.Row
        metrics = ', '.join(f'{field} REAL' for field in usage.METRICS)
        self.conn.execute(f'CREATE TABLE sessions (id TEXT, model TEXT, billing_provider TEXT, started_at REAL, ended_at REAL, last_activity_at REAL, cwd TEXT, title TEXT, {metrics})')
        self.conn.execute(f'CREATE TABLE session_model_usage (session_id TEXT, model TEXT, billing_provider TEXT, {metrics})')
        self.now = datetime(2026, 11, 2, 15, 0).timestamp()

    def tearDown(self):
        self.conn.close()
        self.tmp.cleanup()

    def add(self, sid='s', age=0, cost: float | None = 10, ended=None, last=None, cwd: str | None = '/private/project', **extra):
        values = dict(id=sid, model='gpt-6.1-sol', billing_provider='openai-codex', started_at=self.now-age*86400,
                      ended_at=ended, last_activity_at=self.now-30 if last is None else last, cwd=cwd, title='Session title',
                      input_tokens=100, output_tokens=20, cache_read_tokens=90, cache_write_tokens=0,
                      reasoning_tokens=5, api_call_count=1, estimated_cost_usd=cost, actual_cost_usd=None)
        values.update(extra)
        self.conn.execute(f"INSERT INTO sessions ({','.join(values)}) VALUES ({','.join('?' for _ in values)})", tuple(values.values()))
        self.conn.commit()

    def read(self):
        return usage.dashboard_usage(self.conn, self.now)

    def test_hours_days_and_provider_split_are_session_start_cohorts(self):
        self.add()
        self.add('claude', cost=5, model='claude-opus-4-6', billing_provider='anthropic')
        d = self.read()
        self.assertEqual(len(d['hours']), 24)
        self.assertEqual(len(d['days']), 91)
        self.assertEqual(d['periodBasis'], 'session_started_at')
        p = d['periods']['today']
        self.assertEqual(p['estimatedCostUsd'], 15)
        self.assertEqual(p['byProvider']['claude']['costUsd'], 5)
        self.assertEqual(p['byProvider']['codex']['costUsd'], 10)
        self.assertEqual(sum(h['estimatedCostUsd'] or 0 for h in d['hours']), 15)
        self.assertIn('not per-call', d['note'])

    def test_cache_read_is_a_breakdown_not_extra_tokens(self):
        self.add()
        p = self.read()['periods']['today']
        self.assertEqual(p['totalTokens'], 120)
        self.assertEqual(p['cacheReadShare'], .9)

    def test_unknown_cost_remains_null_and_partial(self):
        self.add(cost=None)
        p = self.read()['periods']['today']
        self.assertIsNone(p['estimatedCostUsd'])
        self.assertTrue(p['partialCost'])
        self.add('known', cost=3)
        self.assertEqual(self.read()['periods']['today']['estimatedCostUsd'], 3)
        self.assertTrue(self.read()['periods']['today']['partialCost'])

    def test_detailed_model_attribution_replaces_session_model_rows(self):
        self.add(cost=99)
        self.conn.execute('INSERT INTO session_model_usage (session_id,model,billing_provider,input_tokens,output_tokens,estimated_cost_usd) VALUES (?,?,?,?,?,?)', ('s', 'claude-sonnet', 'anthropic', 10, 5, 2))
        d = self.read()
        self.assertEqual(d['periods']['today']['estimatedCostUsd'], 2)
        self.assertEqual(d['periods']['today']['models'][0]['name'], 'claude-sonnet')
        self.assertEqual(d['periods']['today']['byProvider']['claude']['costUsd'], 2)

    def test_recency_fences_orphans_ended_and_future_sessions(self):
        self.add('active')
        self.add('orphan', last=self.now-181)
        self.add('ended', ended=self.now-1)
        self.add('future', last=self.now+1)
        self.assertEqual([s['sessionId'] for s in self.read()['active']], ['active'])

    def test_long_lived_recent_session_is_active_but_outside_spend_cohort(self):
        self.add(age=120)
        d = self.read()
        self.assertEqual(len(d['active']), 1)
        self.assertIsNone(d['activity']['estimatedCostUsd'])

    def test_project_is_cwd_basename_then_title_never_full_path(self):
        self.add()
        self.add('title', cwd=None)
        d = self.read()
        self.assertEqual({p['name'] for p in d['periods']['today']['projects']}, {'project', 'Session title'})
        self.assertNotIn('/private', json.dumps(d))
        self.assertEqual(d['projectBasis'], 'cwd_basename_then_title')

    def test_periods_streak_busiest_and_91_day_cutoff(self):
        self.add('today', cost=2)
        self.add('yesterday', age=1, cost=9)
        self.add('week', age=6, cost=4)
        self.add('month', age=29, cost=5)
        self.add('old', age=90, cost=1)
        self.add('outside', age=91, cost=100)
        d = self.read()
        self.assertEqual(d['activity']['streak'], 2)
        self.assertEqual(d['activity']['activeDays'], 5)
        self.assertEqual(d['activity']['estimatedCostUsd'], 21)
        self.assertEqual(d['activity']['busiestDay']['costUsd'], 9)
        self.assertEqual(d['periods']['week']['estimatedCostUsd'], 15)
        self.assertEqual(d['periods']['month']['estimatedCostUsd'], 20)

    def test_empty_database_never_invents_a_price(self):
        d = self.read()
        self.assertIsNone(d['periods']['today']['estimatedCostUsd'])
        self.assertEqual(d['activity']['activeDays'], 0)
        self.assertIsNone(d['activity']['busiestDay'])
        self.assertEqual(d['active'], [])

    def test_read_only_full_projection_preserves_legacy_totals(self):
        self.add()
        before = (self.home / 'state.db').read_bytes()
        with patch.dict(os.environ, {'HERMES_HOME': str(self.home)}), patch.object(usage.time, 'time', return_value=self.now):
            result = usage.get_local_usage()
        self.assertTrue(result['available'])
        self.assertEqual(result['totals']['estimatedCostUsd'], 10)
        self.assertEqual(result['models'][0]['estimatedCostUsd'], 10)
        self.assertEqual(result['dashboard']['periods']['today']['estimatedCostUsd'], 10)
        self.assertEqual(before, (self.home / 'state.db').read_bytes())

    def test_missing_database_is_unavailable_without_creating_it(self):
        missing = self.home / 'missing'
        with patch.dict(os.environ, {'HERMES_HOME': str(missing)}):
            self.assertFalse(usage.get_local_usage()['available'])
        self.assertFalse(missing.exists())

    @unittest.skipUnless(hasattr(time, 'tzset'), 'requires POSIX TZ')
    def test_dst_days_have_real_23_and_25_hour_buckets(self):
        old = os.environ.get('TZ')
        try:
            os.environ['TZ'] = 'America/New_York'
            time.tzset()
            for date, count in ((datetime(2026, 3, 8, 15), 23), (datetime(2026, 11, 1, 15), 25)):
                self.assertEqual(len(usage.dashboard_usage(self.conn, date.timestamp())['hours']), count)
        finally:
            if old is None:
                os.environ.pop('TZ', None)
            else:
                os.environ['TZ'] = old
            time.tzset()


if __name__ == '__main__':
    unittest.main()
