import QtQml
import org.kde.plasma.workspace.dbus as DBus
import "CopyProgress.js" as CopyProgress

// File-copy progress from the island (Settings → notchCopyProgress). The island sends it as a
// D-Bus signal on its /Companion object in whole tens, at most once per 3 s plus the start and
// the end of a copy, so a copy costs a handful of repaints and an idle desktop none: no
// appearance push, no config write. `percent` is -1 while nothing is copied.
QtObject {
    id: source
    property bool active: true
    property int percent: -1
    property QtObject watcher: DBus.SignalWatcher {
        enabled: source.active
        busType: DBus.BusType.Session
        service: "local.hermes.Monolith"
        path: "/Companion"
        iface: "local.hermes.Monolith.Transfers"
        function dbusCopyProgress(value) { source.percent = CopyProgress.percent(value); }
    }
    // A notch that starts (or an island that comes back) during a copy asks once for the
    // current step. No `signature` field: Plasma's module rejects calls that carry one.
    function ask() {
        if (!source.active) return;
        DBus.SessionBus.asyncCall({service: "local.hermes.Monolith", path: "/Companion",
            iface: "local.hermes.Monolith.Transfers", member: "Progress", arguments: []},
            function(reply) { source.percent = CopyProgress.percent(reply); },
            function(error) { source.percent = -1; });
    }
    onActiveChanged: if (active) ask(); else percent = -1
    Component.onCompleted: ask()
}
