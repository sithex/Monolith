// KWin declarative script loaded by the island while automatic game mode is on.
// Tells the island whether the focused window is fullscreen. Purely event-driven: it
// reacts to focus changes and to the focused window's fullScreenChanged, nothing else
// (no timers). The island's own surface, panels, OSDs, popups and notifications never
// count as "the focused window", so opening the island over a game keeps game mode.
// If the island is gone, reports are dropped for a few seconds per failure; the island
// reloads this script when it starts, so nothing is lost by dropping.
import QtQuick
import org.kde.kwin

Item {
    id: root
    property var watched: null
    property real pausedUntil: 0

    DBusCall {
        id: send
        service: "local.hermes.Monolith.GameMode"
        path: "/GameMode"
        dbusInterface: "local.hermes.Monolith.GameMode"
        method: "Fullscreen"
        onFailed: root.pausedUntil = Date.now() + 5000
    }

    function isIsland(w) {
        const names = [String(w.resourceClass || ""), String(w.resourceName || "")];
        return names.some(n => n.toLowerCase() === "hermes-island")
            || String(w.caption || "").indexOf("Hermes Island") === 0;
    }
    function ignored(w) {
        return w.dock || w.notification || w.criticalNotification || w.onScreenDisplay
            || w.popupWindow || w.appletPopup || isIsland(w);
    }
    function report(on) {
        if (Date.now() < root.pausedUntil) return;
        send.arguments = [on];
        send.call();
    }
    function focus(w) {
        if (w && ignored(w)) return;
        root.watched = (w && !w.desktopWindow) ? w : null;
        // Re-sent on every real focus change: focusing the game again closes an island
        // that was opened over it.
        report(!!(root.watched && root.watched.fullScreen));
    }

    Connections {
        target: Workspace
        function onWindowActivated(w) { root.focus(w); }
    }
    Connections {
        target: root.watched
        ignoreUnknownSignals: true
        function onFullScreenChanged() { root.report(!!(root.watched && root.watched.fullScreen)); }
        function onClosed() { root.watched = null; root.report(false); }
    }
    Component.onCompleted: root.focus(Workspace.activeWindow)
}
