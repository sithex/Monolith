#!/usr/bin/python3
"""Physical pointer injection into a *private* nested KWin through its EIS D-Bus endpoint.

Why this exists: XTest on KWin's rootless Xwayland does not move the Wayland pointer, so the
Xvfb-style `xtest_fake_input` clicks are silently dropped. KWin exposes a real libei server
(`org.kde.KWin.EIS.RemoteDesktop.connectToEIS`) on ITS session bus; this helper is a minimal libei
sender that talks to it. It only ever connects to the bus named by `DBUS_SESSION_BUS_ADDRESS`, and
refuses to run unless that is a unix socket inside `HERMES_ISLAND_SCRATCH` (the test sandbox).

Protocol (stdin lines -> one reply line on stdout each):
  move X Y   absolute pointer motion in compositor (logical) coordinates
  down | up  left button press / release
  click      press + release
  type TEXT  physical key presses for [A-Za-z0-9 _-] (shift handled), no Enter ever
  chord N..  press evdev keycodes N.. in order, then release in reverse (e.g. `chord 29 22` = Ctrl+U)
  quit
Startup prints `ready X0 Y0 W H` once absolute-pointer, button AND keyboard devices are resumed.

Phase 2 copy of eis_pointer.py (left untouched for the Phase 1.5 tests) with a keyboard added.
"""
import ctypes
import os
import select
import sys
import time

import gi

gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

BTN_LEFT = 0x110
KEY_LEFTCTRL = 29
KEY_LEFTSHIFT = 42
KEY_SPACE = 57
KEY_MINUS = 12
_ROWS = {16: "qwertyuiop", 30: "asdfghjkl", 44: "zxcvbnm"}
KEYCODES = {}
for _start, _letters in _ROWS.items():
    for _i, _ch in enumerate(_letters):
        KEYCODES[_ch] = _start + _i
for _i, _ch in enumerate("1234567890"):
    KEYCODES[_ch] = 2 + _i
KEYCODES[" "] = KEY_SPACE
KEYCODES["-"] = KEY_MINUS

# enum ei_event_type (libei 1.x)
EI_EVENT_CONNECT = 1
EI_EVENT_DISCONNECT = 2
EI_EVENT_SEAT_ADDED = 3
EI_EVENT_SEAT_REMOVED = 4
EI_EVENT_DEVICE_ADDED = 5
EI_EVENT_DEVICE_REMOVED = 6
EI_EVENT_DEVICE_PAUSED = 7
EI_EVENT_DEVICE_RESUMED = 8

# enum ei_device_capability
CAP_POINTER = 1 << 0
CAP_POINTER_ABSOLUTE = 1 << 1
CAP_KEYBOARD = 1 << 2
CAP_TOUCH = 1 << 3
CAP_SCROLL = 1 << 4
CAP_BUTTON = 1 << 5


def die(message):
    print(f"error {message}", flush=True)
    sys.exit(2)


def guard_private_bus():
    address = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    scratch = os.environ.get("HERMES_ISLAND_SCRATCH", "")
    if not scratch or not address.startswith("unix:path="):
        die("refusing to run without HERMES_ISLAND_SCRATCH and a unix:path= private bus")
    path = os.path.realpath(address[len("unix:path="):])
    if not path.startswith(os.path.realpath(scratch) + os.sep):
        die(f"bus {path} is not inside the scratch sandbox {scratch}")
    return address


def connect_eis(address):
    connection = Gio.DBusConnection.new_for_address_sync(
        address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    # capabilities bitmask of the RemoteDesktop portal: 1 keyboard, 2 pointer, 4 touch (3 = both)
    reply, fds = connection.call_with_unix_fd_list_sync(
        "org.kde.KWin",
        "/org/kde/KWin/EIS/RemoteDesktop",
        "org.kde.KWin.EIS.RemoteDesktop",
        "connectToEIS",
        GLib.Variant("(i)", (3,)),
        GLib.VariantType("(hi)"),
        Gio.DBusCallFlags.NONE,
        10000,
        None,
        None,
    )
    index = reply.unpack()[0]
    return connection, fds.get(index)


def main():
    address = guard_private_bus()
    connection, fd = connect_eis(address)

    ei = ctypes.CDLL("libei.so.1")
    vp = ctypes.c_void_p
    ei.ei_new_sender.restype = vp
    ei.ei_new_sender.argtypes = [vp]
    ei.ei_setup_backend_fd.argtypes = [vp, ctypes.c_int]
    ei.ei_setup_backend_fd.restype = ctypes.c_int
    ei.ei_configure_name.argtypes = [vp, ctypes.c_char_p]
    ei.ei_get_fd.argtypes = [vp]
    ei.ei_dispatch.argtypes = [vp]
    ei.ei_get_event.argtypes = [vp]
    ei.ei_get_event.restype = vp
    ei.ei_event_unref.argtypes = [vp]
    ei.ei_event_get_type.argtypes = [vp]
    ei.ei_event_get_seat.argtypes = [vp]
    ei.ei_event_get_seat.restype = vp
    ei.ei_event_get_device.argtypes = [vp]
    ei.ei_event_get_device.restype = vp
    ei.ei_seat_bind_capabilities.argtypes = [vp, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int]
    ei.ei_device_has_capability.argtypes = [vp, ctypes.c_int]
    ei.ei_device_has_capability.restype = ctypes.c_bool
    ei.ei_device_ref.argtypes = [vp]
    ei.ei_device_ref.restype = vp
    ei.ei_device_start_emulating.argtypes = [vp, ctypes.c_uint32]
    ei.ei_device_get_region.argtypes = [vp, ctypes.c_size_t]
    ei.ei_device_get_region.restype = vp
    for name in ("x", "y", "width", "height"):
        fn = getattr(ei, f"ei_region_get_{name}")
        fn.argtypes = [vp]
        fn.restype = ctypes.c_uint32
    ei.ei_device_pointer_motion_absolute.argtypes = [vp, ctypes.c_double, ctypes.c_double]
    ei.ei_device_button_button.argtypes = [vp, ctypes.c_uint32, ctypes.c_bool]
    ei.ei_device_keyboard_key.argtypes = [vp, ctypes.c_uint32, ctypes.c_bool]
    ei.ei_device_frame.argtypes = [vp, ctypes.c_uint64]
    ei.ei_now.argtypes = [vp]
    ei.ei_now.restype = ctypes.c_uint64

    ctx = ei.ei_new_sender(None)
    ei.ei_configure_name(ctx, b"hermes-island-test")
    if ei.ei_setup_backend_fd(ctx, os.dup(fd.get_fd() if hasattr(fd, "get_fd") else fd)) != 0:
        die("ei_setup_backend_fd failed")
    ei_fd = ei.ei_get_fd(ctx)

    state = {"abs": None, "btn": None, "kbd": None, "region": None, "resumed": set()}
    sequence = [0]

    def pump(timeout=0.0):
        r, _, _ = select.select([ei_fd], [], [], timeout)
        if r:
            ei.ei_dispatch(ctx)
        while True:
            event = ei.ei_get_event(ctx)
            if not event:
                break
            kind = ei.ei_event_get_type(event)
            if kind == EI_EVENT_SEAT_ADDED:
                seat = ei.ei_event_get_seat(event)
                ei.ei_seat_bind_capabilities(seat, CAP_POINTER_ABSOLUTE, CAP_BUTTON, CAP_KEYBOARD, 0, 0)
            elif kind == EI_EVENT_DEVICE_ADDED:
                dev = ei.ei_device_ref(ei.ei_event_get_device(event))
                if ei.ei_device_has_capability(dev, CAP_POINTER_ABSOLUTE):
                    state["abs"] = dev
                    region = ei.ei_device_get_region(dev, 0)
                    if region:
                        state["region"] = tuple(
                            getattr(ei, f"ei_region_get_{n}")(region) for n in ("x", "y", "width", "height")
                        )
                if ei.ei_device_has_capability(dev, CAP_BUTTON):
                    state["btn"] = dev
                if ei.ei_device_has_capability(dev, CAP_KEYBOARD):
                    state["kbd"] = dev
            elif kind == EI_EVENT_DEVICE_RESUMED:
                dev = ei.ei_event_get_device(event)
                for key in ("abs", "btn", "kbd"):
                    if state[key] and dev == state[key] and key not in state["resumed"]:
                        sequence[0] += 1
                        ei.ei_device_start_emulating(dev, sequence[0])
                        state["resumed"].add(key)
            elif kind == EI_EVENT_DISCONNECT:
                die("EIS disconnected")
            ei.ei_event_unref(event)

    deadline = time.time() + 15
    while not ({"abs", "btn", "kbd"} <= state["resumed"] and state["region"]):
        if time.time() > deadline:
            die(f"no absolute pointer+button+keyboard devices from KWin EIS (state={ {k: bool(v) for k, v in state.items()} })")
        pump(0.1)
    print("ready", *state["region"], flush=True)

    def send(dev):
        ei.ei_device_frame(dev, ei.ei_now(ctx))
        pump(0.0)

    def key(code, down):
        ei.ei_device_keyboard_key(state["kbd"], code, down)
        send(state["kbd"])
        time.sleep(0.012)

    def type_text(text):
        for ch in text:
            shifted = ch.isupper() or ch == "_"
            base = "-" if ch == "_" else ch.lower()
            code = KEYCODES.get(base)
            if code is None:
                die(f"unsupported character {ch!r}")
            if shifted:
                key(KEY_LEFTSHIFT, True)
            key(code, True)
            key(code, False)
            if shifted:
                key(KEY_LEFTSHIFT, False)

    for line in sys.stdin:
        parts = line.split()
        if not parts:
            continue
        pump(0.0)
        cmd = parts[0]
        if cmd == "move" and len(parts) == 3:
            ei.ei_device_pointer_motion_absolute(state["abs"], float(parts[1]), float(parts[2]))
            send(state["abs"])
        elif cmd in ("down", "up", "click"):
            for press in {"down": [True], "up": [False], "click": [True, False]}[cmd]:
                ei.ei_device_button_button(state["btn"], BTN_LEFT, press)
                send(state["btn"])
                if cmd == "click":
                    time.sleep(0.03)
        elif cmd == "type" and len(line.rstrip("\n").split(" ", 1)) == 2:
            type_text(line.rstrip("\n").split(" ", 1)[1])
        elif cmd == "chord" and len(parts) >= 2 and all(p.isdigit() for p in parts[1:]):
            codes = [int(p) for p in parts[1:]]
            for code in codes:
                key(code, True)
            for code in reversed(codes):
                key(code, False)
        elif cmd == "quit":
            break
        else:
            print(f"error bad command {line.strip()!r}", flush=True)
            continue
        print("ok", flush=True)
    _ = connection  # keep the bus connection alive for the lifetime of the EIS fd


if __name__ == "__main__":
    main()
