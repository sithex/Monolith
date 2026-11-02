#!/usr/bin/python3
"""Private-bus MPRIS player mock (Phase 6m tests). Refuses any bus that is not the scratch one.

usage: mpris_mock.py SUFFIX        owns org.mpris.MediaPlayer2.SUFFIX on $DBUS_SESSION_BUS_ADDRESS
control: JSON lines on stdin, one "ok" line per command on stdout.

ops: {"op":"set","props":{...},"silent":[names]}   store; PropertiesChanged for the keys not in
                                                  "silent" (Position is never signalled, per spec)
       props: PlaybackStatus "Playing", Rate 1.0, Position <us>, Can* booleans,
              Metadata {"xesam:title": ["s","T"], "mpris:length": ["x",1], ...}
     {"op":"seeked","us":N}                      store Position + emit Seeked
     {"op":"rogue_props","props":{...}}          PropertiesChanged forged by ANOTHER connection
     {"op":"rogue_seeked","us":N}                Seeked forged by another connection
     {"op":"rogue_noc","name":N,"new":""}        unicast NameOwnerChanged from another connection
     {"op":"quit"}                               exit (the name is released)
Every method call a client makes on the Player interface is appended to
$HERMES_ISLAND_SCRATCH/calls-SUFFIX.log as "<Member> <args>".
"""
import json
import os
from pathlib import Path
import sys
import threading
from gi.repository import Gio, GLib

suffix = sys.argv[1]
root = Path(os.environ['HERMES_ISLAND_SCRATCH']).resolve()
assert root.name.startswith('hi-') and root != Path('/')
address = os.environ['DBUS_SESSION_BUS_ADDRESS']
assert address == f'unix:path={root}/sysbus', address
for key in ('HOME', 'XDG_RUNTIME_DIR'):
    assert Path(os.environ[key]).resolve().is_relative_to(root)
NAME = 'org.mpris.MediaPlayer2.' + suffix
PATH = '/org/mpris/MediaPlayer2'
IFACE = 'org.mpris.MediaPlayer2.Player'
CALLS = root / f'calls-{suffix}.log'


def connect():
    return Gio.DBusConnection.new_for_address_sync(
        address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)


bus = connect()
rogue = connect()
V = GLib.Variant
SIGS = {'PlaybackStatus': 's', 'Rate': 'd', 'Position': 'x', 'CanControl': 'b', 'CanPlay': 'b', 'CanPause': 'b',
        'CanSeek': 'b', 'CanGoNext': 'b', 'CanGoPrevious': 'b'}
store = {'PlaybackStatus': V('s', 'Stopped'), 'Rate': V('d', 1.0), 'Position': V('x', 0),
         'Metadata': V('a{sv}', {}), 'CanControl': V('b', True), 'CanPlay': V('b', True), 'CanPause': V('b', True),
         'CanSeek': V('b', True), 'CanGoNext': V('b', True), 'CanGoPrevious': V('b', True)}

XML = ('<node><interface name="org.mpris.MediaPlayer2.Player">'
       '<method name="Play"/><method name="Pause"/><method name="PlayPause"/><method name="Next"/>'
       '<method name="Previous"/><method name="Seek"><arg type="x" direction="in"/></method>'
       '<method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>'
       '<signal name="Seeked"><arg type="x"/></signal>'
       '<property name="PlaybackStatus" type="s" access="read"/><property name="Rate" type="d" access="read"/>'
       '<property name="Metadata" type="a{sv}" access="read"/><property name="Position" type="x" access="read"/>'
       '<property name="CanControl" type="b" access="read"/><property name="CanPlay" type="b" access="read"/>'
       '<property name="CanPause" type="b" access="read"/><property name="CanSeek" type="b" access="read"/>'
       '<property name="CanGoNext" type="b" access="read"/><property name="CanGoPrevious" type="b" access="read"/>'
       '</interface></node>')


def on_method(conn, sender, path, iface, member, params, invocation):
    with CALLS.open('a') as log:
        log.write(f'{member} {params.print_(False)}\n')
    invocation.return_value(None)


def on_get(conn, sender, path, iface, prop):
    return store[prop]


bus.register_object(PATH, Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], on_method, on_get, None)


def typed(props):
    out = {}
    for key, value in props.items():
        if key == 'Metadata':
            out[key] = V('a{sv}', {k: V(sig, val) for k, (sig, val) in value.items()})
        else:
            out[key] = V(SIGS[key], value)
    return out


def emit_props(conn, changed):
    conn.emit_signal(None, PATH, 'org.freedesktop.DBus.Properties', 'PropertiesChanged',
                     V('(sa{sv}as)', (IFACE, changed, [])))
    conn.flush_sync(None)


result = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'RequestName',
                       V('(su)', (NAME, 4)), GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 2000, None)
assert result.unpack() == (1,), result
loop = GLib.MainLoop()
print('ready', bus.get_unique_name(), rogue.get_unique_name(), flush=True)


def control(line):
    cmd = json.loads(line)
    op = cmd['op']
    if op == 'set':
        changed = typed(cmd['props'])
        if 'Metadata' in changed and 'Position' not in changed:
            changed['Position'] = V('x', 0)  # a real player restarts at 0 on a new track
        store.update(changed)
        silent = set(cmd.get('silent', [])) | {'Position'}
        loud = {k: v for k, v in changed.items() if k not in silent}
        if loud:
            emit_props(bus, loud)
    elif op == 'seeked':
        store['Position'] = V('x', cmd['us'])
        bus.emit_signal(None, PATH, IFACE, 'Seeked', V('(x)', (cmd['us'],)))
        bus.flush_sync(None)
    elif op == 'rogue_props':
        emit_props(rogue, typed(cmd['props']))
    elif op == 'rogue_seeked':
        rogue.emit_signal(None, PATH, IFACE, 'Seeked', V('(x)', (cmd['us'],)))
        rogue.flush_sync(None)
    elif op == 'rogue_noc':
        names = rogue.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
                                'ListNames', None, GLib.VariantType.new('(as)'), Gio.DBusCallFlags.NONE, 2000, None)
        for dest in names.unpack()[0]:
            if dest.startswith(':') and dest != rogue.get_unique_name():
                rogue.emit_signal(dest, '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'NameOwnerChanged',
                                  V('(sss)', (cmd['name'], cmd.get('old', ''), cmd.get('new', ''))))
        rogue.flush_sync(None)
    elif op == 'quit':
        print('ok', flush=True)
        os._exit(0)
    else:
        raise AssertionError(line)
    print('ok', flush=True)
    return False


def reader():
    for line in sys.stdin:
        GLib.idle_add(control, line)
    GLib.idle_add(loop.quit)


threading.Thread(target=reader, daemon=True).start()
loop.run()
