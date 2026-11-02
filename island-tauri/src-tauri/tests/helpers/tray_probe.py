#!/usr/bin/python3
"""Private-bus StatusNotifierWatcher and native SNI/DBusMenu probe.

`snapshot` reads actual exported properties, not actor/test-side caches. All menu
activation uses the same DBusMenu Event a desktop host sends to libayatana.
"""
import json
import os
from pathlib import Path
import stat
import sys

from gi.repository import Gio, GLib

root = Path(os.environ["HERMES_ISLAND_SCRATCH"]).resolve(strict=True)
address = os.environ["DBUS_SESSION_BUS_ADDRESS"]
assert address == f"unix:path={root}/bus", "refusing non-private session bus"
assert stat.S_ISSOCK((root / "bus").stat().st_mode)
for key in (
    "HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
    "XDG_STATE_HOME", "XDG_RUNTIME_DIR",
):
    assert Path(os.environ[key]).resolve(strict=True).is_relative_to(root), key
connection = Gio.DBusConnection.new_for_address_sync(
    address,
    Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
    | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
    None, None,
)
NAME = "org.kde.StatusNotifierWatcher"
PATH = "/StatusNotifierWatcher"
IFACE = NAME


def call(service, path, interface, member, parameters):
    return connection.call_sync(
        service, path, interface, member, parameters,
        None, Gio.DBusCallFlags.NONE, 5000, None,
    )


def publish_items(items):
    temporary = root / "tray-items.json.tmp"
    temporary.write_text(json.dumps(items))
    temporary.replace(root / "tray-items.json")


if sys.argv[1] == "watch":
    xml = '''<node><interface name="org.kde.StatusNotifierWatcher">
      <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
      <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
      <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
      <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
      <property name="ProtocolVersion" type="i" access="read"/>
      <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
    </interface></node>'''
    items = []

    def method(conn, sender, path, interface, member, parameters, invocation):
        value = parameters.unpack()[0]
        if member == "RegisterStatusNotifierItem":
            service = sender + value if value.startswith("/") else value + "/StatusNotifierItem"
            if service not in items:
                items.append(service)
            publish_items(items)
            conn.emit_signal(
                None, PATH, IFACE, "StatusNotifierItemRegistered",
                GLib.Variant("(s)", (service,)),
            )
        invocation.return_value(GLib.Variant("()", ()))

    def prop(conn, sender, path, interface, name):
        return {
            "RegisteredStatusNotifierItems": GLib.Variant("as", items),
            "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
            "ProtocolVersion": GLib.Variant("i", 0),
        }[name]

    info = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
    registration = connection.register_object(PATH, info, method, prop, None)
    result = call(
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "RequestName", GLib.Variant("(su)", (NAME, 0)),
    ).unpack()[0]
    assert result == 1
    (root / "tray-watcher-ready").write_text("ready")
    GLib.MainLoop().run()
else:
    items = json.loads((root / "tray-items.json").read_text())
    assert len(items) == 1, items
    service, path = items[0].split("/", 1)
    path = "/" + path

    def item_property(name):
        return call(
            service, path, "org.freedesktop.DBus.Properties", "Get",
            GLib.Variant("(ss)", ("org.kde.StatusNotifierItem", name)),
        ).get_child_value(0).get_variant()

    menu = item_property("Menu").unpack()
    layout = call(
        service, menu, "com.canonical.dbusmenu", "GetLayout",
        GLib.Variant("(iias)", (0, -1, [])),
    ).unpack()[1]
    rows = []

    def walk(node):
        identity, properties, children = node
        if "label" in properties:
            rows.append({
                "id": identity,
                "label": properties["label"].replace("_", "").replace("&", ""),
                "properties": {key: properties[key] for key in (
                    "enabled", "visible", "toggle-type", "toggle-state",
                ) if key in properties},
            })
        for child in children:
            walk(child)

    walk(layout)
    if sys.argv[1] == "dump":
        print(json.dumps(rows, ensure_ascii=False))
    elif sys.argv[1] == "snapshot":
        tooltip = item_property("ToolTip")
        assert tooltip.get_type_string() == "(sa(iiay)ss)", tooltip
        icon_name, pixmaps, title, body = tooltip.unpack()
        owner_pid = call(
            "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "GetConnectionUnixProcessID", GLib.Variant("(s)", (service,)),
        ).unpack()[0]
        print(json.dumps({
            "service": service, "path": path, "ownerPid": owner_pid,
            "rows": rows,
            "tooltip": {"iconName": icon_name, "title": title, "body": body},
        }, ensure_ascii=False))
    else:
        matches = [row for row in rows if row["label"] == sys.argv[1]]
        assert len(matches) == 1, (sys.argv[1], rows)
        assert matches[0]["properties"].get("enabled", True)
        call(
            service, menu, "com.canonical.dbusmenu", "Event",
            GLib.Variant("(isvu)", (
                matches[0]["id"], "clicked", GLib.Variant("i", 0), 0,
            )),
        )
        print("ok")
