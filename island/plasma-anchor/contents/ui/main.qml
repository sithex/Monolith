import QtQuick
import QtQuick.Layouts
import "RequestQueue.js" as Requests
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.private.mpris as Mpris
PlasmoidItem {
    id: root
    property bool online: false
    property date now: new Date()
    property var appearance: ({})
    // Backend pushes only public panel styling through this declared KConfig key.
    property string pushedAppearance: Plasmoid.configuration.notchAppearance || ""
    onPushedAppearanceChanged: {
        if (pushedAppearance) {
            try { appearance = JSON.parse(pushedAppearance); } catch (error) { console.warn("Invalid notch appearance update"); }
        }
    }
    property var requests: Requests.create(function(operation) {backend.connectSource(root.command + " " + operation);})
    property string clockText: Qt.formatTime(now, "hh:mm AP")
    property string dateText: Qt.formatDate(now, "ddd, MMM d")
    property string command: "/usr/bin/python3 /home/nick/Hermes/Ripple/island/anchor-client.py"
    preferredRepresentation: compactRepresentation
    Plasmoid.icon: "utilities-terminal"
    Plasmoid.constraintHints: PlasmaCore.Types.CanFillArea
    // Hover feedback lives inside PanelPill; no tooltip or preview popup.
    toolTipMainText: ""
    toolTipSubText: ""
    // Index zero is Plasma's automatic multiplexer: playing players take precedence.
    // Native D-Bus state, independent of the island's status/style protocol.
    Mpris.Mpris2Model { id: mediaPlayers; currentIndex: 0 }
    Plasma5Support.DataSource {
        id: backend
        engine: "executable"
        onNewData: function(source, data) {
            if (data["exit code"] === undefined) return;
            try {
                var reply = JSON.parse(data.stdout);
                if (reply.online !== undefined) root.online = reply.online;
                if (reply.online) root.appearance = reply;
            } catch (error) { root.online = false; }
            disconnectSource(source);
            root.requests.complete();
        }
    }
    Timer { interval: 15000; running: true; repeat: true; triggeredOnStart: true; onTriggered: root.requests.enqueue("status") }
    Timer { interval: Math.max(100, 60000 - Date.now() % 60000); running: true; onTriggered: { root.now = new Date(); interval = Math.max(100, 60000 - Date.now() % 60000); restart(); } }
    // Settings → Notch → Width (pushed as notchWidth; 144 = the original slot).
    readonly property real notchWidth: Math.max(110, Math.min(320, Number(root.appearance.notchWidth) || 144))
    compactRepresentation: PanelPill {
        Layout.minimumWidth: root.notchWidth
        Layout.maximumWidth: root.notchWidth
        Layout.preferredWidth: root.notchWidth
        clockText: root.clockText
        dateText: root.dateText
        playing: mediaPlayers.currentPlayer !== null && mediaPlayers.currentPlayer.playbackStatus === Mpris.PlaybackStatus.Playing
        showDate: root.appearance.showDate !== false
        stealth: root.appearance.stealth === true
        accent: root.appearance.accent || "#f2cc60"
        background: root.appearance.background || "#171717"
        textColor: root.appearance.text || "#f1ede4"
        dimColor: root.appearance.dim || "#aaa69d"
        borderEnabled: (root.appearance.notchBorder || {}).enabled !== false
        borderColor: (root.appearance.notchBorder || {}).color || accent
        borderOpacity: (root.appearance.notchBorder || {}).opacity ?? 0.1
        borderWidth: (root.appearance.notchBorder || {}).width ?? 0.8
        // Pointer enter/leave animate the native notch only; click remains authoritative.
        onActivated: root.requests.enqueue(root.online ? "pin" : "launch")
    }
    fullRepresentation: compactRepresentation
}
