#!/usr/bin/python3
"""Sole notification server fixture; refuses any non-scratch bus/HOME/XDG env."""
import json
import os
from pathlib import Path
import sys
import threading
from gi.repository import Gio, GLib

root = Path(os.environ['HERMES_ISLAND_SCRATCH']).resolve()
assert root.name.startswith('hi-') and root != Path('/')
assert os.environ['DBUS_SESSION_BUS_ADDRESS'] == f'unix:path={root}/bus'
for key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_RUNTIME_DIR', 'XDG_STATE_HOME'):
    assert Path(os.environ[key]).resolve().is_relative_to(root)
bus = Gio.DBusConnection.new_for_address_sync(os.environ['DBUS_SESSION_BUS_ADDRESS'], Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
xml = '''<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg direction="out" type="as"/></method>
<method name="GetServerInformation"><arg direction="out" type="s"/><arg direction="out" type="s"/><arg direction="out" type="s"/><arg direction="out" type="s"/></method>
<method name="Notify"><arg direction="in" type="s"/><arg direction="in" type="u"/><arg direction="in" type="s"/><arg direction="in" type="s"/><arg direction="in" type="s"/><arg direction="in" type="as"/><arg direction="in" type="a{sv}"/><arg direction="in" type="i"/><arg direction="out" type="u"/></method>
<method name="CloseNotification"><arg direction="in" type="u"/></method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>'''
name = 'org.freedesktop.Notifications'
path = '/org/freedesktop/Notifications'
counter = 0
calls = []
closed = []

def save():
    data = root / 'mock-state.json'
    temp = root / 'mock-state.tmp'
    temp.write_text(json.dumps({'calls': calls, 'closed': closed}))
    temp.replace(data)

def emit(signal, signature, values):
    bus.emit_signal(None, path, name, signal, GLib.Variant(signature, values))
    bus.flush_sync(None)

def method(conn, sender, obj, iface, member, params, invocation):
    global counter
    args = params.unpack()
    if member == 'Notify':
        counter += 1
        notification_id = args[1] or counter
        calls.append({'app': args[0], 'title': args[3], 'id': notification_id})
        save()
        invocation.return_value(GLib.Variant('(u)', (notification_id,)))
    elif member == 'GetCapabilities':
        invocation.return_value(GLib.Variant('(as)', (['body', 'actions'],)))
    elif member == 'GetServerInformation':
        invocation.return_value(GLib.Variant('(ssss)', ('Private mock', 'Hermes tests', '1', '1.2')))
    elif member == 'CloseNotification':
        closed.append(args[0])
        save()
        invocation.return_value(GLib.Variant('()', ()))
        emit('NotificationClosed', '(uu)', (args[0], 3))

bus.register_object(path, Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0], method, None, None)
result = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'RequestName', GLib.Variant('(su)', (name, 4)), GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 2000, None)
assert result.unpack() == (1,)
save()
loop = GLib.MainLoop()
print('ready', bus.get_unique_name(), flush=True)

def control(line):
    parts = line.split()
    if parts[0] == 'closed':
        emit('NotificationClosed', '(uu)', (int(parts[1]), 2))
    elif parts[0] == 'action':
        emit('ActionInvoked', '(us)', (int(parts[1]), 'default'))
    elif parts[0] == 'quit':
        loop.quit()
    else:
        raise AssertionError(line)
    print('ok', flush=True)
    return False

def reader():
    for line in sys.stdin:
        GLib.idle_add(control, line)
threading.Thread(target=reader, daemon=True).start()
loop.run()
bus.close_sync(None)
