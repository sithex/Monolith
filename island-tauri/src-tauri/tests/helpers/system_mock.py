#!/usr/bin/python3
"""Private system-bus mock for UPower or BlueZ (Phase 6a tests). Refuses non-scratch envs.

usage: system_mock.py upower|bluez     (control: JSON lines on stdin, "ok" per line on stdout)

ops: {"op":"props","path":P,"iface":I,"changed":{K:[SIG,VAL]}}          store + PropertiesChanged
     {"op":"emit_props","path":P,"iface":I,"changed":{...}}             PropertiesChanged only (not stored)
     {"op":"rogue_props","path":P,"iface":I,"changed":{...}}            same, from a DIFFERENT connection
     {"op":"added","path":P,"ifaces":{I:{K:[SIG,VAL]}},"signal_path":S}  store + InterfacesAdded (default S="/")
     {"op":"rogue_added","path":P,"ifaces":{...}}                       InterfacesAdded from another connection
     {"op":"removed","path":P,"ifaces":[I],"signal_path":S}              InterfacesRemoved (default S="/")
     {"op":"rogue_noc","name":N,"new":""}                                 unicast NameOwnerChanged from another connection
     {"op":"quit"}
"""
import json
import os
from pathlib import Path
import sys
import threading
from gi.repository import Gio, GLib

kind = sys.argv[1]
assert kind in ('upower', 'bluez')
root = Path(os.environ['HERMES_ISLAND_SCRATCH']).resolve()
assert root.name.startswith('hi-') and root != Path('/')
address = os.environ['DBUS_SYSTEM_BUS_ADDRESS']
assert address == f'unix:path={root}/sysbus', address
for key in ('HOME', 'XDG_RUNTIME_DIR'):
    assert Path(os.environ[key]).resolve().is_relative_to(root)


def connect():
    return Gio.DBusConnection.new_for_address_sync(
        address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)


bus = connect()
rogue = connect()
V = GLib.Variant
store = {}  # path -> iface -> prop -> GLib.Variant

if kind == 'upower':
    NAME = 'org.freedesktop.UPower'
    ROOT, DISPLAY, BAT0 = ('/org/freedesktop/UPower', '/org/freedesktop/UPower/devices/DisplayDevice',
                           '/org/freedesktop/UPower/devices/battery_BAT0')
    DEV = 'org.freedesktop.UPower.Device'
    store[ROOT] = {NAME: {'OnBattery': V('b', True)}}
    store[DISPLAY] = {DEV: {'IsPresent': V('b', True), 'Percentage': V('d', 55.0), 'State': V('u', 2),
                            'TimeToEmpty': V('x', 7200), 'TimeToFull': V('x', 0)}}
    store[BAT0] = {DEV: {'IsPresent': V('b', True), 'Percentage': V('d', 77.0), 'State': V('u', 2),
                         'TimeToEmpty': V('x', 100), 'TimeToFull': V('x', 0)}}
    xml_root = '<node><interface name="org.freedesktop.UPower"><property name="OnBattery" type="b" access="read"/></interface></node>'
    xml_dev = ('<node><interface name="org.freedesktop.UPower.Device"><property name="IsPresent" type="b" access="read"/>'
               '<property name="Percentage" type="d" access="read"/><property name="State" type="u" access="read"/>'
               '<property name="TimeToEmpty" type="x" access="read"/><property name="TimeToFull" type="x" access="read"/>'
               '</interface></node>')

    def get_property(conn, sender, path, iface, prop):
        return store[path][iface][prop]

    bus.register_object(ROOT, Gio.DBusNodeInfo.new_for_xml(xml_root).interfaces[0], None, get_property, None)
    for p in (DISPLAY, BAT0):
        bus.register_object(p, Gio.DBusNodeInfo.new_for_xml(xml_dev).interfaces[0], None, get_property, None)
else:
    NAME = 'org.bluez'
    ADAPTER, DEVICE, DEVICE2 = '/org/bluez/hci0', '/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF', '/org/bluez/hci0/dev_11_22_33_44_55_66'
    store[ADAPTER] = {'org.bluez.Adapter1': {'Powered': V('b', True), 'Discovering': V('b', False), 'Alias': V('s', 'mock-adapter')}}
    store[DEVICE] = {
        'org.bluez.Device1': {'Address': V('s', 'AA:BB:CC:DD:EE:FF'), 'Alias': V('s', 'Mock Headset'), 'Name': V('s', 'Headset'),
                              'Icon': V('s', 'audio-headset'), 'Paired': V('b', True), 'Connected': V('b', True),
                              'Trusted': V('b', True), 'Adapter': V('o', ADAPTER)},
        'org.bluez.Battery1': {'Percentage': V('y', 80)}}
    store[DEVICE2] = {'org.bluez.Device1': {'Address': V('s', '11:22:33:44:55:66'), 'Alias': V('s', 'Mock Mouse'),
                                            'Paired': V('b', True), 'Connected': V('b', False), 'Adapter': V('o', ADAPTER)}}
    xml = ('<node><interface name="org.freedesktop.DBus.ObjectManager"><method name="GetManagedObjects">'
           '<arg direction="out" type="a{oa{sa{sv}}}"/></method>'
           '<signal name="InterfacesAdded"><arg type="o"/><arg type="a{sa{sv}}"/></signal>'
           '<signal name="InterfacesRemoved"><arg type="o"/><arg type="as"/></signal></interface></node>')

    def method(conn, sender, obj, iface, member, params, invocation):
        assert member == 'GetManagedObjects'
        invocation.return_value(V('(a{oa{sa{sv}}})', (store,)))

    bus.register_object('/', Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0], method, None, None)


def variants(changed):
    return {k: V(sig, val) for k, (sig, val) in changed.items()}


def emit_props(conn, path, iface, changed):
    conn.emit_signal(None, path, 'org.freedesktop.DBus.Properties', 'PropertiesChanged',
                     V('(sa{sv}as)', (iface, variants(changed), [])))
    conn.flush_sync(None)


def emit_added(conn, signal_path, path, ifaces):
    conn.emit_signal(None, signal_path, 'org.freedesktop.DBus.ObjectManager', 'InterfacesAdded',
                     V('(oa{sa{sv}})', (path, {i: variants(p) for i, p in ifaces.items()})))
    conn.flush_sync(None)


result = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'RequestName',
                       V('(su)', (NAME, 4)), GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 2000, None)
assert result.unpack() == (1,), result
loop = GLib.MainLoop()
print('ready', bus.get_unique_name(), rogue.get_unique_name(), flush=True)


def control(line):
    cmd = json.loads(line)
    op = cmd['op']
    if op == 'props':
        for key, (sig, val) in cmd['changed'].items():
            store[cmd['path']][cmd['iface']][key] = V(sig, val)
        emit_props(bus, cmd['path'], cmd['iface'], cmd['changed'])
    elif op == 'emit_props':
        emit_props(bus, cmd['path'], cmd['iface'], cmd['changed'])
    elif op == 'rogue_props':
        emit_props(rogue, cmd['path'], cmd['iface'], cmd['changed'])
    elif op == 'added':
        store.setdefault(cmd['path'], {})
        for iface, props in cmd['ifaces'].items():
            store[cmd['path']].setdefault(iface, {}).update(variants(props))
        emit_added(bus, cmd.get('signal_path', '/'), cmd['path'], cmd['ifaces'])
    elif op == 'rogue_added':
        emit_added(rogue, '/', cmd['path'], cmd['ifaces'])
    elif op == 'removed':
        for iface in cmd['ifaces']:
            store.get(cmd['path'], {}).pop(iface, None)
        bus.emit_signal(None, cmd.get('signal_path', '/'), 'org.freedesktop.DBus.ObjectManager',
                        'InterfacesRemoved', V('(oas)', (cmd['path'], cmd['ifaces'])))
        bus.flush_sync(None)
    elif op == 'rogue_noc':
        # A peer unicasts NameOwnerChanged(name, old, new) to every OTHER unique name (the
        # island's connection among them). zbus cannot client-filter a well-known sender, so
        # only the in-code sender check stands between this and a spoofed name loss.
        names = rogue.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
                                'ListNames', None, GLib.VariantType.new('(as)'), Gio.DBusCallFlags.NONE, 2000, None)
        for dest in names.unpack()[0]:
            if dest.startswith(':') and dest != rogue.get_unique_name():
                rogue.emit_signal(dest, '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'NameOwnerChanged',
                                  V('(sss)', (cmd['name'], cmd.get('old', ''), cmd.get('new', ''))))
        rogue.flush_sync(None)
    elif op == 'quit':
        loop.quit()
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
bus.close_sync(None)
rogue.close_sync(None)
