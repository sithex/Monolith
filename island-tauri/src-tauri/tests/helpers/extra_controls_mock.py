#!/usr/bin/python3
"""Private PPD/NetworkManager/ScreenSaver/KWin fixtures. No host service access."""
import json
import os
from pathlib import Path
import sys
import threading
from gi.repository import Gio, GLib

root = Path(os.environ['HERMES_ISLAND_SCRATCH']).resolve()
address = os.environ['DBUS_SESSION_BUS_ADDRESS']
assert root.name.startswith('hi-bus-') and address == f'unix:path={root}/sysbus'
assert Path(os.environ['HOME']).resolve().is_relative_to(root)
V = GLib.Variant
PPD = 'org.freedesktop.UPower.PowerProfiles'
NM = 'org.freedesktop.NetworkManager'
SS = 'org.freedesktop.ScreenSaver'
KWIN = 'org.kde.KWin'
paths = {PPD:'/org/freedesktop/UPower/PowerProfiles', NM:'/org/freedesktop/NetworkManager', SS:'/org/freedesktop/ScreenSaver', KWIN:'/ColorPicker'}
ifaces = {PPD:PPD, NM:NM, SS:SS, KWIN:'org.kde.kwin.ColorPicker'}

def connect():
    return Gio.DBusConnection.new_for_address_sync(address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)

bus, rogue = connect(), connect()
props = {
    PPD:{'ActiveProfile':V('s','balanced'),'Profiles':V('aa{sv}',[{'Profile':V('s',p),'Driver':V('s','fixture')} for p in ['power-saver','balanced','performance']]),'PerformanceDegraded':V('s',''),'ActiveProfileHolds':V('aa{sv}',[])},
    NM:{'State':V('u',70),'Connectivity':V('u',4),'PrimaryConnectionType':V('s','802-3-ethernet'),'WirelessEnabled':V('b',False),'WirelessHardwareEnabled':V('b',False),'Devices':V('ao',[])},
    SS:{}, KWIN:{},
}
echo = True
fail = False
cookie = 6
cookies = set()

def record(path, member, args):
    with (root/'extra-calls.jsonl').open('a') as out:
        out.write(json.dumps([path,member,list(args)])+'\n')

def property_signal(name, conn=bus):
    conn.emit_signal(None, paths[name], 'org.freedesktop.DBus.Properties', 'PropertiesChanged', V('(sa{sv}as)', (ifaces[name],props[name],[])))
    conn.flush_sync(None)

def get_property(conn,sender,path,iface,prop):
    record(path,'Get',[iface,prop])
    if iface == 'org.freedesktop.NetworkManager.Device':
        return V('u',2)
    return props[iface][prop]

def set_property(conn,sender,path,iface,prop,value):
    record(path,'Set:'+prop,[value.unpack()])
    assert iface == PPD and prop == 'ActiveProfile'
    if fail:
        return False
    if echo:
        props[PPD][prop] = value
        property_signal(PPD)
    return True

def method(conn,sender,path,iface,member,params,invocation):
    global cookie
    args = params.unpack()
    record(path,member,args)
    if fail:
        invocation.return_dbus_error('org.kde.Error.Fixture','fixture failure')
    elif member == 'Inhibit':
        cookie += 1
        cookies.add(cookie)
        invocation.return_value(V('(u)',(cookie,)))
    elif member == 'UnInhibit':
        assert args[0] in cookies
        cookies.remove(args[0])
        invocation.return_value(V('()',()))
    elif member == 'pick':
        invocation.return_value(V('(u)',(0xff12ab34,)))
    else:
        raise AssertionError(member)

for name in props:
    properties = ''.join(f'<property name="{key}" type="{value.get_type_string()}" access="{"readwrite" if key == "ActiveProfile" else "read"}"/>' for key,value in props[name].items())
    methods = ''
    if name == SS:
        methods = '<method name="Inhibit"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="u" direction="out"/></method><method name="UnInhibit"><arg type="u" direction="in"/></method>'
    if name == KWIN:
        methods = '<method name="pick"><arg type="u" direction="out"/></method>'
    xml = f'<node><interface name="{ifaces[name]}">{properties}{methods}</interface></node>'
    bus.register_object(paths[name],Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0],method,get_property,set_property)
xml = '<node><interface name="org.freedesktop.NetworkManager.Device"><property name="DeviceType" type="u" access="read"/></interface></node>'
bus.register_object(paths[NM]+'/Devices/1',Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0],method,get_property,None)

def name_call(member,name):
    signature = '(su)' if member == 'RequestName' else '(s)'
    args = (name,4) if member == 'RequestName' else (name,)
    return bus.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus',member,V(signature,args),GLib.VariantType.new('(u)'),Gio.DBusCallFlags.NONE,2000,None).unpack()

for name in props:
    assert name_call('RequestName',name) == (1,)
loop = GLib.MainLoop()
print('ready',bus.get_unique_name(),rogue.get_unique_name(),flush=True)

def control(line):
    global echo,fail
    cmd = json.loads(line)
    op = cmd['op']
    if op == 'set':
        name = cmd['name']
        props[name][cmd['property']] = V(cmd['signature'],cmd['value'])
        if cmd.get('emit',True):
            property_signal(name)
    elif op == 'rogue_properties':
        property_signal(cmd['name'],rogue)
    elif op == 'rogue_noc':
        peers = rogue.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','ListNames',None,GLib.VariantType.new('(as)'),Gio.DBusCallFlags.NONE,2000,None).unpack()[0]
        for peer in peers:
            if peer.startswith(':') and peer != rogue.get_unique_name():
                rogue.emit_signal(peer,'/org/freedesktop/DBus','org.freedesktop.DBus','NameOwnerChanged',V('(sss)',(cmd['name'],bus.get_unique_name(),'')))
        rogue.flush_sync(None)
    elif op == 'release':
        name_call('ReleaseName',cmd['name'])
    elif op == 'acquire':
        assert name_call('RequestName',cmd['name']) == (1,)
    elif op == 'echo':
        echo = cmd['on']
    elif op == 'fail':
        fail = cmd['on']
    elif op == 'assert_cookies':
        assert len(cookies) == cmd['count'], cookies
    elif op == 'quit':
        loop.quit()
    else:
        raise AssertionError(cmd)
    print('ok',flush=True)
    return False

def reader():
    for line in sys.stdin:
        GLib.idle_add(control,line)
    GLib.idle_add(loop.quit)
threading.Thread(target=reader,daemon=True).start()
loop.run()
bus.close_sync(None)
rogue.close_sync(None)
