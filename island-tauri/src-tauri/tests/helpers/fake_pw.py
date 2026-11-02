#!/usr/bin/python3
"""Fake pw-record / pw-dump / pactl for the equalizer tests (invoked through symlinks named
after the tool). Configuration comes from FAKE_PW_* variables, which the test passes
explicitly. Nothing here touches audio, PipeWire or the network.

pw-record: appends its argv to $FAKE_PW_ARGV (one line per start), then streams a stereo f32le
           sine ($FAKE_PW_HZ, default 1000) to stdout in real time until killed (or exits after
           $FAKE_PW_EXIT_AFTER_MS).
pw-dump:   prints $FAKE_PW_DUMP (default "[]").
pactl:     'get-default-sink' prints $FAKE_PW_SINK (default "fake-sink").
"""
import math
import os
import struct
import sys
import time

tool = os.path.basename(sys.argv[0])
if tool == 'pw-dump':
    sys.stdout.write(os.environ.get('FAKE_PW_DUMP', '[]'))
elif tool == 'pactl':
    if sys.argv[1:] == ['get-default-sink']:
        print(os.environ.get('FAKE_PW_SINK', 'fake-sink'))
    else:
        sys.exit(2)
elif tool == 'pw-record':
    with open(os.environ['FAKE_PW_ARGV'], 'a') as log:
        log.write(' '.join(sys.argv[1:]) + '\n')
    hz = float(os.environ.get('FAKE_PW_HZ', '1000'))
    flutter = os.environ.get('FAKE_PW_FLUTTER') == '1'
    exit_after = float(os.environ.get('FAKE_PW_EXIT_AFTER_MS', '0')) / 1000.0
    rate, chunk = 48000, 480  # 10 ms of audio per write
    start = time.monotonic()
    n = 0
    phase = 0.0
    out = sys.stdout.buffer
    while True:
        samples = []
        for i in range(chunk):
            # Flutter: the frequency sweeps slowly (+-50 %), so the energy keeps moving across
            # bands and EVERY analysed frame differs (a steady tone converges to one frame and
            # would hide an unthrottled emit rate).
            f = hz * (1 + 0.5 * math.sin(2 * math.pi * 0.7 * (n + i) / rate)) if flutter else hz
            phase += 2 * math.pi * f / rate
            v = 0.5 * math.sin(phase)
            samples.extend((v, v))
        n += chunk
        try:
            out.write(struct.pack('<%df' % len(samples), *samples))
            out.flush()
        except BrokenPipeError:
            break
        if exit_after and time.monotonic() - start > exit_after:
            break
        delay = start + n / rate - time.monotonic()
        if delay > 0:
            time.sleep(delay)
else:
    sys.exit(3)
