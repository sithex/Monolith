"""Read-only metadata accounting. Never selects transcripts, prompts or credentials.

The 30-day cohort is sessions STARTED in the period, not per-call usage in it.
Cached/reasoning counters are breakdowns; totalTokens is input + output only.
Costs are recorded local USD accounting, not account credits or subscription spend.
"""
import json
import os
from pathlib import Path
import sqlite3
import time
import math
from datetime import datetime, timedelta
from typing import Any

METRICS = {
    'input_tokens': 'inputTokens', 'output_tokens': 'outputTokens',
    'cache_read_tokens': 'cacheReadTokens', 'cache_write_tokens': 'cacheWriteTokens',
    'reasoning_tokens': 'reasoningTokens', 'api_call_count': 'apiCallCount',
    'estimated_cost_usd': 'estimatedCostUsd', 'actual_cost_usd': 'actualCostUsd',
}

def aggregate(conn, source, group=False):
    columns = ['COUNT(DISTINCT session_id) AS sessions']
    columns += [f'SUM({field}) AS {name}' for field, name in METRICS.items()]
    if group:
        columns += ["COALESCE(model, 'Unknown') AS model", "COALESCE(billing_provider, '') AS provider"]
    sql = 'SELECT ' + ', '.join(columns) + ' FROM (' + source + ')'
    if group:
        sql += ' GROUP BY model, billing_provider ORDER BY SUM(input_tokens) + SUM(output_tokens) DESC'
    rows = []
    for row in conn.execute(sql, (time.time() - 30 * 86400,)):
        result = dict(row)
        for name in METRICS.values():
            if not name.endswith('CostUsd'):
                result[name] = result[name] or 0
        result['totalTokens'] = result['inputTokens'] + result['outputTokens']
        rows.append(result)
    return rows

def clean_number(value):
    return value if isinstance(value, (int, float)) and math.isfinite(value) and value >= 0 else None


def family(provider, model=''):
    name = (provider + ' ' + model).lower()
    if 'anthropic' in name or 'claude' in name:
        return 'claude'
    if 'openai' in name or 'codex' in name or 'gpt-' in name:
        return 'codex'
    return 'other'


def summarize(rows):
    """Unknown counters remain unknown; partial prices are explicitly tagged."""
    result: dict[str, Any] = {'sessions': len({r['sessionId'] for r in rows})}
    for name in METRICS.values():
        values = [clean_number(r.get(name)) for r in rows]
        result[name] = sum(v for v in values if v is not None) if any(v is not None for v in values) else None
        if not name.endswith('CostUsd') and any(v is None for v in values):
            result[name] = None
    result['totalTokens'] = (result['inputTokens'] + result['outputTokens']
                             if result['inputTokens'] is not None and result['outputTokens'] is not None else None)
    result['partialCost'] = any(clean_number(r.get('estimatedCostUsd')) is None for r in rows)
    denominator = result['inputTokens']
    result['cacheReadShare'] = (min(1, result['cacheReadTokens'] / denominator)
                               if denominator and result['cacheReadTokens'] is not None else None)
    result['byProvider'] = {p: summarize_provider(rows, p) for p in ('claude', 'codex', 'other')}
    return result


def summarize_provider(rows, provider):
    selected = [r for r in rows if r['family'] == provider]
    values = [clean_number(r.get('estimatedCostUsd')) for r in selected]
    tokens = [clean_number(r.get('inputTokens')) for r in selected]
    outputs = [clean_number(r.get('outputTokens')) for r in selected]
    return {'costUsd': sum(v for v in values if v is not None) if any(v is not None for v in values) else None,
            'tokens': sum(v for v in tokens + outputs if v is not None) if any(v is not None for v in tokens + outputs) else None}


def dashboard_usage(conn, now):
    """Session-start cohorts, NEVER pretend cumulative rows are timestamped calls.

    state.db has no per-call cost ledger. Both sessions and session_model_usage
    contain lifetime cumulative counters. first_seen/last_seen do not timestamp
    individual costs. Keep this limitation on the wire and in the dashboard UI.
    """
    today = datetime.fromtimestamp(now).date()
    first = today - timedelta(days=90)
    cutoff = datetime.combine(first, datetime.min.time()).timestamp()
    columns = {r[1] for r in conn.execute('PRAGMA table_info(sessions)')}
    optional = ', '.join(name if name in columns else f'NULL AS {name}'
                         for name in ('cwd', 'title', 'last_activity_at', 'ended_at'))
    fields = ', '.join(METRICS)
    recency = 'last_activity_at' if 'last_activity_at' in columns else 'NULL'
    sessions = [dict(r) for r in conn.execute(
        f'SELECT id, model, billing_provider, started_at, {optional}, {fields} FROM sessions WHERE (started_at >= ? OR {recency} >= ?) AND started_at <= ?', (cutoff, now - 180, now))]
    detailed = bool(conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_model_usage'").fetchone())
    details = {}
    if detailed:
        for row in conn.execute(f'''SELECT u.session_id, u.model, u.billing_provider, {', '.join('u.' + f for f in METRICS)}
                                   FROM session_model_usage u JOIN sessions s ON s.id=u.session_id
                                   WHERE s.started_at >= ? AND s.started_at <= ?''', (cutoff, now)):
            details.setdefault(row['session_id'], []).append(dict(row))
    rows, active = [], []
    for session in sessions:
        project = (Path(session['cwd']).name if session.get('cwd') else '') or session.get('title') or 'Unknown project'
        project = project[:120]
        for detail in details.get(session['id'], [session]) if session['started_at'] >= cutoff else []:
            row: dict[str, Any] = {name: clean_number(detail.get(field)) for field, name in METRICS.items()}
            row.update(sessionId=session['id'], model=detail.get('model') or 'Unknown model',
                       provider=detail.get('billing_provider') or '', project=project,
                       startedAtS=session['started_at'], date=datetime.fromtimestamp(session['started_at']).date().isoformat())
            row['family'] = family(row['provider'], row['model'])
            rows.append(row)
        last = clean_number(session.get('last_activity_at'))
        if session.get('ended_at') is None and last is not None and 0 <= now - last <= 180:
            active.append(dict(sessionId=session['id'], project=project, model=session['model'] or 'Unknown model',
                               provider=session['billing_provider'] or '', family=family(session['billing_provider'] or '', session['model'] or ''),
                               startedAtS=session['started_at'], lastActivityAtS=last,
                               outputTokens=clean_number(session['output_tokens']), costUsd=clean_number(session['estimated_cost_usd'])))
    days = []
    for index in range(91):
        date = (first + timedelta(days=index)).isoformat()
        days.append(dict(date=date, **summarize([r for r in rows if r['date'] == date])))
    hours = []
    start = datetime.combine(today, datetime.min.time()).timestamp()
    end = datetime.combine(today + timedelta(days=1), datetime.min.time()).timestamp()
    while start < end:
        hours.append(dict(startS=start, **summarize([r for r in rows if start <= r['startedAtS'] < start + 3600])))
        start += 3600
    periods = {}
    for length, key in ((1, 'today'), (7, 'week'), (30, 'month')):
        date = (today - timedelta(days=length - 1)).isoformat()
        selected = [r for r in rows if r['date'] >= date]
        shares = {}
        for field in ('model', 'project'):
            shares[field] = [dict(name=name, family=next((r['family'] for r in selected if r[field] == name), 'other'),
                                 **summarize([r for r in selected if r[field] == name])) for name in sorted({r[field] for r in selected})]
            shares[field].sort(key=lambda r: r['estimatedCostUsd'] if r['estimatedCostUsd'] is not None else -1, reverse=True)
        periods[key] = dict(**summarize(selected), models=shares['model'][:20], projects=shares['project'][:20])
    streak = 0
    for day in reversed(days):
        if day['sessions'] == 0:
            break
        streak += 1
    priced = [d for d in days if d['estimatedCostUsd'] is not None]
    busiest = max(priced, key=lambda d: d['estimatedCostUsd']) if priced else None
    return dict(periodBasis='session_started_at', periods=periods, hours=hours, days=days,
                active=sorted(active, key=lambda s: s['lastActivityAtS'], reverse=True)[:20],
                activity=dict(**summarize(rows), activeDays=sum(d['sessions'] > 0 for d in days), streak=streak,
                              busiestDay=None if busiest is None else dict(date=busiest['date'], costUsd=busiest['estimatedCostUsd'])),
                projectBasis='cwd_basename_then_title',
                note='API-equivalent estimates · session-start cohorts, not per-call calendar spend.')


def read_local_usage():
    home = Path(os.environ.get('HERMES_HOME') or Path.home() / '.hermes').expanduser().resolve()
    with sqlite3.connect((home / 'state.db').as_uri() + '?mode=ro', uri=True, timeout=2) as conn:
        conn.row_factory = sqlite3.Row
        conn.execute('PRAGMA query_only = ON')
        fields = ', '.join(METRICS)
        source = f'SELECT id AS session_id, model, billing_provider, {fields} FROM sessions WHERE started_at >= ?'
        model_source = source
        if conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_model_usage'").fetchone():
            # Detailed attribution is authoritative for sessions with model rows.
            # Legacy/cumulative sessions lacking those rows retain their session counters.
            detail_fields = ', '.join('u.' + field for field in METRICS)
            model_source = f'''WITH recent AS (
                SELECT id AS session_id, model, billing_provider, {fields}
                FROM sessions WHERE started_at >= ?
            ) SELECT u.session_id, u.model, u.billing_provider, {detail_fields}
              FROM session_model_usage u JOIN recent r ON r.session_id = u.session_id
              UNION ALL SELECT * FROM recent r WHERE NOT EXISTS (
                SELECT 1 FROM session_model_usage u WHERE u.session_id = r.session_id
              )'''
        return dict(available=True, periodDays=30, periodBasis='session_started_at',
                    totals=aggregate(conn, source)[0], models=aggregate(conn, model_source, True),
                    dashboard=dashboard_usage(conn, time.time()), unavailableReason=None)

def get_local_usage():
    try:
        return read_local_usage()
    except (sqlite3.Error, OSError, ValueError):
        return dict(available=False, periodDays=30, periodBasis='session_started_at',
                    totals=None, models=[],
                    unavailableReason='Local usage database is missing, busy, unreadable, or has an unsupported schema.')

if __name__ == '__main__':
    print(json.dumps(get_local_usage(), allow_nan=False))
