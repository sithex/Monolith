#!/usr/bin/python3
"""Fake-only Controls executables. No subprocesses or host service connections exist here."""
import fcntl
import json
import os
from pathlib import Path
import socket
import sys

root = Path(os.environ['HERMES_ISLAND_CONTROL_FIXTURE']).resolve()
assert str(root).startswith(os.environ.get('TMPDIR', '/nonexistent')) or 'hi-' in root.name
assert Path(os.environ['HOME']).resolve() == root / 'home'
program = Path(sys.argv[0]).name
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as calls:
    fcntl.flock(calls, fcntl.LOCK_EX)
    calls.write(json.dumps([program, args, os.environ.get('LC_ALL'), os.environ['HOME']]) + '\n')

if args == ['subscribe']:
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    sub.bind(str(root / f'sub-{os.getpid()}'))
    sub.settimeout(.1)
    try:
        while not (root / 'offline').exists():
            try:
                event = sub.recv(4096)
            except socket.timeout:
                continue
            sys.stdout.buffer.write(event)
            sys.stdout.buffer.flush()
    finally:
        sub.close()
        (root / f'sub-{os.getpid()}').unlink(missing_ok=True)
    sys.exit(1)
if (root / 'offline').exists():
    sys.exit(1)
if (root / 'hang').exists():
    import time
    time.sleep(120)
if (root / 'oversize').exists():
    sys.stdout.write('x' * (9 * 1024 * 1024))
    sys.exit(0)

with (root / 'lock').open('a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    state = json.loads((root / 'state.json').read_text())
    if program == 'pactl':
        if args == ['-f', 'json', 'list', 'sinks']:
            print(json.dumps(state['sinks']))
        elif args == ['-f', 'json', 'list', 'sink-inputs']:
            print(json.dumps(state['streams']))
        elif args == ['get-default-sink']:
            print(state['default'])
        elif args == ['get-source-mute', '@DEFAULT_SOURCE@']:
            print('Mute: ' + ('yes' if state.get('mic') else 'no'))
        elif args[:2] == ['set-source-mute', '@DEFAULT_SOURCE@'] and len(args) == 3:
            state['mic'] = not state.get('mic', False) if args[2] == 'toggle' else args[2] == '1'
        elif len(args) == 2 and args[0] == 'set-default-sink':
            assert any(s['name'] == args[1] for s in state['sinks'])
            state['default'] = args[1]
        elif len(args) == 3 and args[0] in ('set-sink-volume', 'set-sink-mute', 'set-sink-input-volume', 'set-sink-input-mute', 'move-sink-input'):
            sink = args[0].startswith('set-sink-') and not args[0].startswith('set-sink-input-')
            records = state['sinks'] if sink else state['streams']
            record = next(r for r in records if str(r['name'] if sink else r['index']) == args[1])
            if args[0].endswith('volume'):
                value = round(int(args[2].removesuffix('%')) * 65536 / 100)
                record['volume'] = {'mono': {'value': value}}
            elif args[0] == 'move-sink-input':
                record['sink'] = next(r['index'] for r in state['sinks'] if r['name'] == args[2])
            else:
                record['mute'] = args[2] == '1'
        else:
            raise AssertionError(('unexpected pactl argv', args))
    elif program == 'tailscale':
        assert args == ['status', '--json']
        print(json.dumps(state['tailscale']))
    elif program == 'hermes-fish':
        if not args:
            print('\n'.join('  ' + name + ('  [active]' if name == state['voice'] else '') for name in state['voices']))
        elif args == ['current']:
            print(state['voice'])
        else:
            assert len(args) == 1 and args[0] in state['voices']
            state['voice'] = args[0]
    elif program == 'spectacle':
        assert args == ['-r']
    elif program in ('checkupdates', 'paru'):
        assert args == ([] if program == 'checkupdates' else ['-Qua'])
        result = state[program]
        sys.stdout.write(result.get('stdout', ''))
        sys.stderr.write(result.get('stderr', ''))
        sys.exit(result.get('code', 0))
    else:
        raise AssertionError(program)
    (root / 'state.json').write_text(json.dumps(state))
