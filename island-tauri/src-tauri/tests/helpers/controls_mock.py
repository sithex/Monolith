#!/usr/bin/python3
"""Phase 6b2 private PowerDevil mock. No user bus, desktop or external executables."""
import json
import os
from pathlib import Path
import sys
import threading
from gi.repository import Gio, GLib

root = Path(os.environ['HERMES_ISLAND_SCRATCH']).resolve()
assert root.name.startswith('hi-bus-')
address = os.environ['DBUS_SESSION_BUS_ADDRESS']
assert address == f'unix:path={root}/sysbus'
for key in ('HOME', 'XDG_RUNTIME_DIR'):
    assert Path(os.environ[key]).resolve().is_relative_to(root)
V = GLib.Variant
NAME = 'org.kde.ScreenBrightness'
ROOT = '/org/kde/ScreenBrightness'
DISPLAY = NAME + '.Display'

def connect():
    return Gio.DBusConnection.new_for_address_sync(address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)

bus, rogue = connect(), connect()
props = {'display0': {'Label': V('s', 'Fixture monitor'), 'IsInternal': V('b', False),
                     'MaxBrightness': V('i', 10000), 'Brightness': V('i', 5000)}}
names = ['display0']
fail_writes = False
echo = True
root_xml = '''<node><interface name="org.kde.ScreenBrightness">
<property name="DisplaysDBusNames" type="as" access="read"/>
<method name="AdjustBrightnessStepWithContext"><arg type="u" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>'''
display_xml = '''<node><interface name="org.kde.ScreenBrightness.Display">
<property name="Label" type="s" access="read"/><property name="IsInternal" type="b" access="read"/>
<property name="MaxBrightness" type="i" access="read"/><property name="Brightness" type="i" access="read"/>
<method name="SetBrightnessWithContext"><arg type="i" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>'''

def signal(conn, member, sig, args, path=ROOT):
    conn.emit_signal(None, path, NAME, member, V(sig, args))
    conn.flush_sync(None)

def get_property(conn, sender, path, iface, prop):
    if path == ROOT:
        return V('as', names)
    return props[path.rsplit('/', 1)[1]][prop]

def method(conn, sender, path, iface, member, params, invocation):
    args = params.unpack()
    with (root / 'controls-calls.jsonl').open('a') as f:
        f.write(json.dumps([path, member, list(args)]) + '\n')
    if fail_writes:
        invocation.return_dbus_error('org.kde.Error.Fixture', 'fixture write failure')
        return
    if member == 'SetBrightnessWithContext':
        name = path.rsplit('/', 1)[1]
        if echo:
            props[name]['Brightness'] = V('i', args[0])
            signal(bus, 'BrightnessChanged', '(siss)', (name, args[0], sender, args[2]))
    invocation.return_value(V('()', ()))

bus.register_object(ROOT, Gio.DBusNodeInfo.new_for_xml(root_xml).interfaces[0], method, get_property, None)
for name in ['display0', 'display1']:
    props.setdefault(name, dict(props['display0']))
    bus.register_object(ROOT + '/' + name, Gio.DBusNodeInfo.new_for_xml(display_xml).interfaces[0], method, get_property, None)
result = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'RequestName',
    V('(su)', (NAME, 4)), GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 2000, None)
assert result.unpack() == (1,)
loop = GLib.MainLoop()
print('ready', bus.get_unique_name(), rogue.get_unique_name(), flush=True)

def control(line):
    global echo, fail_writes, names
    cmd = json.loads(line)
    op = cmd['op']
    if op == 'emit':
        conn = rogue if cmd.get('rogue') else bus
        signal(conn, cmd['member'], cmd['signature'], tuple(cmd['args']), cmd.get('path', ROOT))
    elif op == 'names':
        names = cmd['names']
    elif op == 'echo':
        echo = cmd['on']
    elif op == 'fail':
        fail_writes = cmd['on']
    elif op == 'rogue_noc':
        peers = rogue.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'ListNames',
            None, GLib.VariantType.new('(as)'), Gio.DBusCallFlags.NONE, 2000, None).unpack()[0]
        for peer in peers:
            if peer.startswith(':') and peer != rogue.get_unique_name():
                rogue.emit_signal(peer, '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'NameOwnerChanged', V('(sss)', (NAME, bus.get_unique_name(), '')))
        rogue.flush_sync(None)
    elif op == 'quit':
        loop.quit()
    else:
        raise AssertionError(cmd)
    print('ok', flush=True)
    return False

def reader():
    for line in sys.stdin:
        GLib.idle_add(control, line)
    GLib.idle_add(loop.quit)

threading.Thread(target=reader, daemon=True).start()
loop.run()
bus.close_sync(None)
rogue.close_sync(None)
