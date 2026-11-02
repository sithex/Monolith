#!/usr/bin/python3
"""Back-to-back compositor captures (org.kde.KWin.ScreenShot2.CaptureArea) of a *private* nested KWin.

Phase 2.5 spike helper. Like kwin_capture.py it refuses to touch any bus that is not a unix socket
inside the scratch sandbox (HERMES_ISLAND_SCRATCH), so it can never capture the real session.

Protocol (stdin lines -> exactly one reply line on stdout each):
  burst X Y W H N OUTDIR   capture the area N times back to back; every DISTINCT frame i is written to
                           OUTDIR/frame_<i:03>.raw (raw KWin bytes, stride as reported); reply is one
                           JSON line {format,stride,width,height,captures,frames:[{i,t0,t1,last_t1,seen}]}
                           where t0/t1 are time.time() seconds immediately before/after the CaptureArea
                           call that first returned that frame, last_t1 the last one that still did.
  quit
The reply for `burst` is only sent after all frames are written, so the caller can start its
animation right after sending `burst` and read the timeline afterwards.
"""
import fcntl
import gc
import hashlib
import json
import os
import sys
import threading
import time

import gi

gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

F_SETPIPE_SZ = 1031


def die(message):
    print(json.dumps({"error": message}), flush=True)
    sys.exit(2)


def guard_private_bus():
    address = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    scratch = os.environ.get("HERMES_ISLAND_SCRATCH", "")
    if not scratch or not address.startswith("unix:path="):
        die("refusing: need HERMES_ISLAND_SCRATCH and a unix:path= private bus")
    path = os.path.realpath(address[len("unix:path="):])
    if not path.startswith(os.path.realpath(scratch) + os.sep):
        die(f"bus {path} is not inside the scratch sandbox")
    return address


def capture_area(connection, x, y, w, h):
    read_fd, write_fd = os.pipe()
    try:
        fcntl.fcntl(write_fd, F_SETPIPE_SZ, 1 << 20)
    except OSError:
        pass  # a smaller pipe only makes the reader thread matter more
    chunks = []

    def drain():
        while True:
            data = os.read(read_fd, 1 << 20)
            if not data:
                break
            chunks.append(data)
        os.close(read_fd)

    reader = threading.Thread(target=drain)
    reader.start()
    fds = Gio.UnixFDList()
    index = fds.append(write_fd)
    options = {"native-resolution": GLib.Variant("b", True), "include-cursor": GLib.Variant("b", False)}
    t0 = time.time()
    try:
        reply, _ = connection.call_with_unix_fd_list_sync(
            "org.kde.KWin.ScreenShot2",
            "/org/kde/KWin/ScreenShot2",
            "org.kde.KWin.ScreenShot2",
            "CaptureArea",
            GLib.Variant("(iiuua{sv}h)", (x, y, w, h, options, index)),  # CaptureArea(x,y,w,h,options,pipe)
            GLib.VariantType("(a{sv})"),
            Gio.DBusCallFlags.NONE,
            20000,
            fds,
            None,
        )
    except Exception as error:  # noqa: BLE001 - reported to the test
        os.close(write_fd)
        reader.join(5)
        die(f"CaptureArea failed: {error}")
    t1 = time.time()
    os.close(write_fd)
    del fds
    gc.collect()
    reader.join(30)
    if reader.is_alive():
        die("capture pipe never reached EOF")
    meta = {k: v for k, v in reply.unpack()[0].items() if k in ("width", "height", "stride", "format")}
    return b"".join(chunks), meta, t0, t1


def main():
    address = guard_private_bus()
    connection = Gio.DBusConnection.new_for_address_sync(
        address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    print("ready", flush=True)
    for line in sys.stdin:
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "quit":
            break
        if parts[0] == "burst" and len(parts) == 7 and all(p.lstrip("-").isdigit() for p in parts[1:6]):
            x, y, w, h, n = (int(p) for p in parts[1:6])
            out = parts[6]
            real = os.path.realpath(out)
            if not real.startswith(os.path.realpath(os.environ["HERMES_ISLAND_SCRATCH"]) + os.sep):
                die(f"output dir {real} is not inside the scratch sandbox")
            os.makedirs(real, exist_ok=True)
            # KWin renders the scene on demand (~1 ms per small area), i.e. a burst samples the
            # *committed* client state far faster than the 60 Hz output. Only distinct frames are
            # kept, each with the first/last time it was seen and how often.
            meta, distinct, last = {}, [], None
            for _ in range(n):
                data, meta, t0, t1 = capture_area(connection, x, y, w, h)
                digest = hashlib.blake2b(data, digest_size=16).digest()
                if digest == last and distinct:
                    distinct[-1]["last_t1"] = t1
                    distinct[-1]["seen"] += 1
                    continue
                last = digest
                distinct.append({"i": len(distinct), "t0": t0, "t1": t1, "last_t1": t1, "seen": 1, "data": data})
                if len(distinct) > 400:
                    die("more than 400 distinct frames in one burst")
            frames = []
            for frame in distinct:
                with open(os.path.join(real, f"frame_{frame['i']:03}.raw"), "wb") as handle:
                    handle.write(frame.pop("data"))
                frames.append(frame)
            meta["captures"] = n
            meta["frames"] = frames
            print(json.dumps(meta), flush=True)
        else:
            print(json.dumps({"error": f"bad command {line.strip()!r}"}), flush=True)


if __name__ == "__main__":
    main()
