#!/usr/bin/env python3
"""Exercise npm run dev over loopback without starting a browser."""
import os
import pathlib
import signal
import subprocess
import time
import urllib.request
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/dev-smoke.log'

def main():
    job = subprocess.Popen(['nice', '-n', '10', 'npm', 'run', 'dev'], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        for _ in range(80):
            if job.poll() is not None:
                raise RuntimeError('Dev server exited before becoming ready; refusing another server')
            try:
                with urllib.request.urlopen('http://127.0.0.1:5197/', timeout=.5) as response:
                    page = response.read().decode()
                    assert response.status == 200 and 'src/gallery.tsx' in page
                with urllib.request.urlopen('http://127.0.0.1:5197/src/gallery.tsx', timeout=1) as response:
                    assert 'Hermes Island' in response.read().decode()
                print('PASS: npm run dev → 127.0.0.1:5197 HTTP 200, actual gallery module served; no browser started')
                break
            except OSError:
                time.sleep(.1)
        else:
            raise RuntimeError('Dev server readiness deadline')
    finally:
        try:
            os.killpg(job.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        output, _ = job.communicate(timeout=5)
        OUT.parent.mkdir(exist_ok=True)
        OUT.write_text(output)
if __name__ == '__main__':
    main()
