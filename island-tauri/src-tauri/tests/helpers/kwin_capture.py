#!/usr/bin/python3
"""Capture the *composited* workspace of a private nested KWin (org.kde.KWin.ScreenShot2).

Writes the raw pixels to argv[1] and prints one JSON line with KWin's metadata (width, height,
stride, QImage format). Like `eis_pointer.py` it refuses to touch any bus that is not a unix socket
inside the scratch sandbox (`HERMES_ISLAND_SCRATCH`), so it can never capture the real session.
"""
import gc
import json
import os
import sys
import threading

import gi

gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib  # noqa: E402


def die(message):
    print(json.dumps({"error": message}), flush=True)
    sys.exit(2)


def main():
    address = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    scratch = os.environ.get("HERMES_ISLAND_SCRATCH", "")
    if not scratch or not address.startswith("unix:path="):
        die("refusing: need HERMES_ISLAND_SCRATCH and a unix:path= private bus")
    bus_path = os.path.realpath(address[len("unix:path="):])
    if not bus_path.startswith(os.path.realpath(scratch) + os.sep):
        die(f"bus {bus_path} is not inside the scratch sandbox")
    if len(sys.argv) != 2:
        die("usage: kwin_capture.py OUT.raw")

    connection = Gio.DBusConnection.new_for_address_sync(
        address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    read_fd, write_fd = os.pipe()
    chunks = []

    def drain():
        while True:
            data = os.read(read_fd, 1 << 20)  # returns what is available; b"" only at EOF
            if not data:
                break
            chunks.append(data)
        os.close(read_fd)

    reader = threading.Thread(target=drain)
    reader.start()
    fds = Gio.UnixFDList()
    index = fds.append(write_fd)
    options = {
        "native-resolution": GLib.Variant("b", True),
        "include-cursor": GLib.Variant("b", False),
    }
    try:
        reply, _ = connection.call_with_unix_fd_list_sync(
            "org.kde.KWin.ScreenShot2",
            "/org/kde/KWin/ScreenShot2",
            "org.kde.KWin.ScreenShot2",
            "CaptureWorkspace",
            GLib.Variant("(a{sv}h)", (options, index)),
            GLib.VariantType("(a{sv})"),
            Gio.DBusCallFlags.NONE,
            20000,
            fds,
            None,
        )
    except Exception as error:  # noqa: BLE001 - report any D-Bus failure to the test
        os.close(write_fd)
        reader.join(5)
        die(f"CaptureWorkspace failed: {error}")
    # EOF needs EVERY write end closed: ours, and the dup that the UnixFDList still owns.
    os.close(write_fd)
    del fds
    gc.collect()
    reader.join(60)
    if reader.is_alive():
        die("capture pipe never reached EOF")
    meta = {k: v for k, v in reply.unpack()[0].items() if k in ("width", "height", "stride", "format", "scale", "type")}
    data = b"".join(chunks)
    with open(sys.argv[1], "wb") as out:
        out.write(data)
    meta["bytes"] = len(data)
    print(json.dumps(meta), flush=True)


if __name__ == "__main__":
    main()
