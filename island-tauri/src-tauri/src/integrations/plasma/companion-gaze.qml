// KWin declarative script loaded by the island while the notch companion is on.
// Forwards the pointer position to the island at most ~30 times a second, only while
// the pointer actually moves (a trailing single-shot timer, never a free-running poll).
// Nothing is sent while a fullscreen window (a game, a video) has focus: the notch is
// hidden then anyway. If the island is gone, sending pauses for a few seconds per failure.
import QtQuick
import org.kde.kwin

Item {
    id: root
    property real lastX: -1
    property real lastY: -1
    property real pausedUntil: 0

    DBusCall {
        id: send
        service: "local.hermes.Monolith"
        path: "/Companion"
        dbusInterface: "local.hermes.Monolith.Companion"
        method: "Pointer"
        onFailed: root.pausedUntil = Date.now() + 5000
    }
    Timer {
        id: tick
        interval: 33
        onTriggered: {
            if (Date.now() < root.pausedUntil) return;
            const active = Workspace.activeWindow;
            if (active && active.fullScreen) return;
            const p = Workspace.cursorPos;
            if (p.x === root.lastX && p.y === root.lastY) return;
            root.lastX = p.x;
            root.lastY = p.y;
            send.arguments = [Math.round(p.x), Math.round(p.y)];
            send.call();
        }
    }
    Connections {
        target: Workspace
        function onCursorPosChanged() { if (!tick.running) tick.start(); }
    }
}
