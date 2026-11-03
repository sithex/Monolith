"""Narrow user-local hover/click bridge for this repository's Plasma pill."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import socket
ROOT = Path(__file__).resolve().parent.parent

def online():
    try:
        state = json.loads((ROOT / '.island-data-overlay/status.json').read_text())
        os.kill(state['pid'], 0)
        return time.time() - state['updated'] < 8
    except (OSError, ValueError, KeyError):
        return False

def request(command):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(1)
        client.connect(str(ROOT / '.island-data-overlay/panel.sock'))
        client.sendall((command + '\n').encode())
        data = b''
        while b'\n' not in data:
            part = client.recv(4096)
            if not part or len(data) > 16384:
                raise OSError('Invalid panel reply')
            data += part
        return json.loads(data)


if __name__ == '__main__':
    command = sys.argv[1:]
    if len(command) != 1 or command[0] not in {'status', 'launch', 'enter', 'leave', 'pin'}:
        raise SystemExit('Expected status, launch, enter, leave or pin')
    operation = command[0]
    try:
        if operation == 'launch':
            if not online():
                # Own systemd scope: the island must not live in plasmashell's cgroup,
                # or restarting/crashing Plasma takes it down too.
                argv = [str(ROOT / 'island-tauri/run.sh'), '--overlay']
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
                    if request('status').get('online'):
                        break
                except (OSError, ValueError):
                    pass
                time.sleep(0.1)
            operation = 'pin'
        print(json.dumps(request(operation)))
    except (OSError, ValueError):
        print(json.dumps({'online': False}))
