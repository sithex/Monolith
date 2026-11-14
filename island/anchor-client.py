"""Narrow user-local hover/click bridge for this repository's Plasma pill."""
# Every notch click starts this script, so it imports only what a click needs (no pathlib,
# subprocess only for launch, json only where a reply must be inspected); the widget also
# runs it with `python3 -I -S`. The island's reply is already one JSON line and is passed through.
import os
import sys
import time
import socket
ROOT = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))
DATA = os.path.join(ROOT, '.island-data-overlay')

def resolve_data_dirs(env=None, root=ROOT, overlay=True):
    """Read-only policy shared with Rust and run.sh; sockets keep their existing path."""
    env = os.environ if env is None else env
    home = env.get('HOME', '/')
    if not os.path.isabs(home):
        home = '/'
    def base(key, default):
        value = env.get(key, '')
        return os.path.join(value if os.path.isabs(value) else os.path.join(home, default), 'monolith')
    config = base('XDG_CONFIG_HOME', '.config')
    state = base('XDG_STATE_HOME', '.local/state')
    old = os.path.join(root, '.island-data-overlay' if overlay else '.island-data')
    def has_data(path):
        try:
            with os.scandir(path) as entries:
                return any(e.name != 'anchor-client.py' or not e.is_symlink() for e in entries)
        except FileNotFoundError:
            return False
        except OSError:
            return True
    pending = not os.path.isfile(os.path.join(state, 'MIGRATED-FROM-REPO.json')) and any(os.path.lexists(os.path.join(p, '.migration-pending')) for p in (config, state))
    if os.path.isdir(old) and (os.path.exists(os.path.join(old, '.xdg-migration-failed')) or pending or (not has_data(config) and not has_data(state))):
        config = state = old
    return config, state, old

def online():
    import json
    try:
        with open(os.path.join(resolve_data_dirs()[1], 'status.json')) as file:
            state = json.load(file)
        os.kill(state['pid'], 0)
        return time.time() - state['updated'] < 8
    except (OSError, ValueError, KeyError):
        return False

def request(command):
    """Send one command; return the island's raw one-line JSON reply."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(1)
        client.connect(os.path.join(DATA, 'panel.sock'))
        client.sendall((command + '\n').encode())
        data = b''
        while b'\n' not in data:
            part = client.recv(4096)
            if not part or len(data) > 16384:
                raise OSError('Invalid panel reply')
            data += part
        return data.split(b'\n', 1)[0].decode()


def parse_args(args):
    if len(args) not in (1, 2) or args[0] not in {'status', 'launch', 'enter', 'leave', 'pin', 'moved'}:
        raise ValueError('Expected status, launch, enter, leave, moved or pin [widget-id]')
    source = ''
    if len(args) == 2:
        if args[0] not in {'pin', 'launch'} or not args[1].isascii() or not args[1].isdigit() or not 0 < int(args[1]) <= 4294967295:
            raise ValueError('Invalid notch widget id')
        source = ' ' + args[1]
    return args[0], source

if __name__ == '__main__':
    command = sys.argv[1:]
    try:
        operation, source = parse_args(command)
    except ValueError as error:
        raise SystemExit(str(error))
    try:
        if operation == 'launch':
            if not online():
                import subprocess
                # Own systemd scope: the island must not live in plasmashell's cgroup,
                # or restarting/crashing Plasma takes it down too.
                argv = [os.path.join(ROOT, 'island-tauri', 'run.sh'), '--overlay']
                scoped = ['systemd-run', '--user', '--scope', '--collect', '--quiet', '--'] + argv
                try:
                    subprocess.Popen(scoped, cwd=ROOT, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     start_new_session=True)
                except OSError:
                    subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     start_new_session=True)
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                try:
                    import json
                    if json.loads(request('status')).get('online'):
                        break
                except (OSError, ValueError):
                    pass
                time.sleep(0.1)
            operation = 'pin'
        print(request(operation + source))
    except (OSError, ValueError):
        print('{"online": false}')
