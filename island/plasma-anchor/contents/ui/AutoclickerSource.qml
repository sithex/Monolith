import QtQml
import org.kde.plasma.workspace.dbus as DBus

// Whether the island's autoclicker is clicking. The island sends a D-Bus signal on its
// /Companion object when a run starts and when it stops (never per click), so an idle
// desktop costs nothing and a run costs two repaints.
QtObject {
    id: source
    property bool clicking: false
    property QtObject watcher: DBus.SignalWatcher {
        busType: DBus.BusType.Session
        service: "local.hermes.Monolith"
        path: "/Companion"
        iface: "local.hermes.Monolith.Autoclicker"
        function dbusClickingChanged(value) { source.clicking = value === true; }
    }
    // A notch that starts (or an island that comes back) mid-run asks once. No `signature`
    // field: Plasma's module rejects calls that carry one.
    function ask() {
        DBus.SessionBus.asyncCall({service: "local.hermes.Monolith", path: "/Companion",
            iface: "local.hermes.Monolith.Autoclicker", member: "Clicking", arguments: []},
            function(reply) { source.clicking = reply === true; },
            function(error) { source.clicking = false; });
    }
    Component.onCompleted: ask()
}
