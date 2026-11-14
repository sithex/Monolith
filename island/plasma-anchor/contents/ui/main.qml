import QtQuick
import QtQuick.Layouts
import "RequestQueue.js" as Requests
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.private.mpris as Mpris
import org.kde.plasma.workspace.dbus as DBus
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
    // Stable per-user link to the checkout's client (kept current by the island; created
    // once by the install step). No machine-specific repository path lives in the widget.
    property string command: "case \"$XDG_STATE_HOME\" in /*) MONOLITH_STATE_HOME=\"$XDG_STATE_HOME\";; *) MONOLITH_STATE_HOME=\"$HOME/.local/state\";; esac; /usr/bin/python3 -I -S \"${MONOLITH_STATE_HOME:-$HOME/.local/state}/monolith/anchor-client.py\""
    preferredRepresentation: compactRepresentation
    Plasmoid.icon: "utilities-terminal"
    Plasmoid.constraintHints: Plasmoid.CanFillArea
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
    // Plasma docks a floating panel against the screen edge while a window is maximized (and
    // floats it again after), shifting this widget by the float margin. Tell the island once
    // the slide has finished so it re-reads the notch position; nothing is polled.
    readonly property int displayHints: Plasmoid.containmentDisplayHints
    onDisplayHintsChanged: if (root.online) panelSettled.restart()
    // Resolution and scale changes can move the notch even without a floatingness change.
    onScreenGeometryChanged: if (root.online) panelSettled.restart()
    Timer { id: panelSettled; interval: 450; onTriggered: root.requests.enqueue("moved") }
    // Online/offline used to be polled: `anchor-client.py status` every 15 s per notch, i.e. a
    // python start (~13 ms) three times per 15 s on a three-monitor desktop, forever. The island
    // owns the D-Bus name below for as long as it runs, so its appearance and disappearance are
    // events; one status read follows each (late enough for status.json to be fresh or stale),
    // plus a slow safety net for an island that runs without the name.
    DBus.DBusServiceWatcher {
        id: islandName
        busType: DBus.BusType.Session
        watchedService: "local.hermes.Monolith"
        onRegisteredChanged: {
            if (registered) { islandUp.restart(); islandUpLate.restart(); copySource.ask(); }
            else { islandUp.stop(); islandUpLate.stop(); islandGone.restart(); copySource.percent = -1; }
        }
    }
    // Just launched: its first status write lands within a second or two.
    Timer { id: islandUp; interval: 1500; onTriggered: root.requests.enqueue("status") }
    Timer { id: islandUpLate; interval: 6000; onTriggered: root.requests.enqueue("status") }
    // Gone: status.json counts as stale after 8 s; ask once it has.
    Timer { id: islandGone; interval: 9000; onTriggered: root.requests.enqueue("status") }
    Timer { id: statusSafetyNet; objectName: "statusSafetyNet"; interval: 120000; running: true; repeat: true; triggeredOnStart: true; onTriggered: root.requests.enqueue("status") }
    Timer { interval: Math.max(100, 60000 - Date.now() % 60000); running: true; onTriggered: { root.now = new Date(); interval = Math.max(100, 60000 - Date.now() % 60000); restart(); } }
    // Settings → Notch → Width (pushed as notchWidth; 144 = the original slot).
    readonly property real notchWidth: Math.max(110, Math.min(320, Number(root.appearance.notchWidth) || 144))
    // Live states pushed with the appearance; ignored once the island is gone so a stale
    // KConfig value can never leave the notch pulsing.
    readonly property var live: root.online ? (root.appearance.live || {}) : ({})
    // Notch companion (Settings → Appearance → Notch). The island relays the global pointer
    // as a D-Bus signal sent only while the pointer moves; nothing here polls.
    readonly property bool companion: root.appearance.companion !== false
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    // Game mode may freeze the companion (and the notch's own animations); no pointer data is
    // wanted then, so the relay's signals aren't even delivered to this process.
    readonly property bool frozen: root.appearance.companionFrozen === true
    DBus.SignalWatcher {
        enabled: root.companion && !root.frozen
        busType: DBus.BusType.Session
        service: "local.hermes.Monolith"
        path: "/Companion"
        iface: "local.hermes.Monolith.Companion"
        function dbusGaze(x, y) {
            root.pointerX = Number(x) || 0;
            root.pointerY = Number(y) || 0;
            root.pointerAt = Date.now();
        }
    }
    // File-copy progress (Settings → notchCopyProgress): a D-Bus signal from the island, in
    // steps; see CopyProgressSource.qml.
    readonly property bool copyProgress: root.appearance.copyProgress !== false
    CopyProgressSource { id: copySource; active: root.copyProgress }
    // Drop shelf (Settings → shelfEnabled): dropped file URLs go to the island over D-Bus,
    // never through the helper's shell command line. See ShelfSink.qml.
    readonly property bool shelf: root.appearance.shelf !== false
    ShelfSink { id: shelfSink }
    compactRepresentation: PanelPill {
        id: notchPill
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
        working: root.live.working === true
        waiting: root.live.waiting === true
        waitingPulse: root.appearance.waitingPulse !== false
        hoverPeek: root.appearance.hoverPeek !== false
        hoverDelay: Number(root.appearance.hoverDelay ?? 180)
        peekTrack: mediaPlayers.currentPlayer ? [mediaPlayers.currentPlayer.track, mediaPlayers.currentPlayer.artist].filter(s => !!s).join(" — ") : ""
        micMuted: root.live.micMuted === true
        copyPercent: root.online && root.copyProgress ? copySource.percent : -1
        shelfEnabled: root.online && root.shelf
        onFilesDropped: urls => shelfSink.add(urls, function(count) { notchPill.shelfResult(count); })
        companion: root.companion
        companionCharacter: String(root.appearance.companionCharacter || "blob")
        companionReactive: root.appearance.companionReactive !== false
        companionFrozen: root.frozen
        companionMonitor: String(root.appearance.companionMonitor || "")
        pointerX: root.pointerX
        pointerY: root.pointerY
        pointerAt: root.pointerAt
        timerEndsAt: (root.live.timer && !root.live.timer.paused) ? Number(root.live.timer.endsAt) || 0 : 0
        timerPausedMs: (root.live.timer && root.live.timer.paused) ? Number(root.live.timer.remainingMs) || 0 : -1
        // Pointer enter/leave animate the native notch only; click remains authoritative.
        onActivated: root.requests.enqueue((root.online ? "pin" : "launch") + " " + Plasmoid.id)
    }
    fullRepresentation: compactRepresentation
}
