"""Read-only agent metadata adapter; executed as python3 -I -c, never imported from HOME."""
import json
import pathlib
import sqlite3
import sys

path = pathlib.Path(sys.argv[1])
now = float(sys.argv[2])
# mode=ro refuses a missing DB rather than silently creating it. query_only also prevents
# accidental writes through any later query change. No journal mode or schema is changed.
connection = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=0.5)
connection.execute('PRAGMA query_only=ON')
connection.execute('PRAGMA trusted_schema=OFF')
assert connection.execute('PRAGMA query_only').fetchone() == (1,)
columns = ['id', 'source', 'parent_session_id', 'model', 'billing_provider', 'started_at',
           'ended_at', 'end_reason', 'last_activity_at', 'api_call_count', 'title']
keys = ['id', 'source', 'parentSessionId', 'model', 'billingProvider', 'startedAt',
        'endedAt', 'endReason', 'lastActivityAt', 'apiCallCount', 'title']
rows = connection.execute('SELECT ' + ','.join(columns) +
    ' FROM sessions WHERE started_at >= ? ORDER BY started_at DESC LIMIT 10001', (now - 86400,)).fetchall()
if len(rows) > 10000:
    raise ValueError('session row limit')
for row in rows:
    if any(isinstance(v, str) and len(v.encode('utf-8')) > 4096 for v in row):
        raise ValueError('session field limit')
print(json.dumps([dict(zip(keys, row)) for row in rows], allow_nan=False))
connection.close()
