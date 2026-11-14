import QtQuick
import QtQuick.Shapes
import QtQuick.Window
import org.kde.kirigami as Kirigami
import "Shelf.js" as Shelf

// Native, panel-contained notch. Hover never opens a webview or changes actor mode.
Item {
    id: pill
    property string clockText: ""
    property string dateText: ""
    property bool showDate: true
    property bool playing: false
    property bool stealth: false
    property color accent: "#f2cc60"
    property color background: "#171717"
    property color textColor: "#f1ede4"
    property color dimColor: "#aaa69d"
    property bool borderEnabled: true
    property color borderColor: accent
    property real borderOpacity: 0.1
    property real borderWidth: 0.8
    // Live states (Dynamic-Island style). All default to "nothing to show".
    property bool working: false
    property bool waiting: false        // Hermes waits for an approval/answer
    property bool waitingPulse: true    // Settings → notchWaitingPulse
    property bool micMuted: false
    // Game mode (gameModeFreezeCompanion, delivered as `companionFrozen`): nothing in the notch
    // animates or ticks in the background; every held state stays on screen.
    property bool companionFrozen: false
    // Waiting has no palette colour of its own (the scheme has only an error red), so it is a
    // fixed amber that reads against any accent; it takes precedence over the working pulse.
    readonly property color alertColor: "#f5a623"
    readonly property bool waitingShown: waiting && waitingPulse
    readonly property bool pulsing: waitingShown || working
    readonly property color pulseColor: waitingShown ? alertColor : accent
    // Notch companion: a small face at the left whose eyes follow the global pointer.
    property bool companion: false
    property string companionCharacter: "blob"
    property bool companionReactive: true
    // Output name that shows the companion ("" = every notch). Screen.name is the KScreen
    // output name (DP-1, …) on Wayland; tests override `screenName`.
    property string companionMonitor: ""
    property string screenName: Screen.name
    readonly property bool companionHere: companionMonitor === "" || companionMonitor === screenName
    // Hover peek (Settings → notchHoverPeek): "Title — Artist" from main.qml's MPRIS model.
    property bool hoverPeek: true
    property real hoverDelay: 180
    property string peekTrack: ""
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    // On a narrow notch the mic mark needs the companion's room.
    readonly property bool companionShown: companion && companionHere && !(micMuted && width < 140)
    property real timerEndsAt: 0       // Unix ms while a timer runs, else 0
    property real timerPausedMs: -1    // remaining ms while paused, else -1
    property real nowMs: Date.now()
    readonly property real timerLeftMs: timerEndsAt > 0 ? Math.max(0, timerEndsAt - nowMs) : timerPausedMs
    readonly property bool timerShown: timerEndsAt > 0 ? timerLeftMs > 0 : timerPausedMs >= 0
    readonly property string timerText: {
        const total = Math.ceil(Math.max(0, timerLeftMs) / 1000);
        const h = Math.floor(total / 3600), m = Math.floor(total % 3600 / 60), s = total % 60;
        const pad = n => (n < 10 ? "0" : "") + n;
        return (h > 0 ? h + ":" + pad(m) : m) + ":" + pad(s);
    }
    // File-copy progress: -1 = none, else 0..100 in tens (main.qml, from the island's D-Bus
    // signal). Static text on the date line, after a timer. A notch on a monitor without the
    // pointer (companion on) or a frozen notch holds still: "copying", without the number.
    property int copyPercent: -1
    readonly property bool copyShown: copyPercent >= 0
    readonly property bool copyLive: animateHere && !companionFrozen
    readonly property string copyText: !copyShown ? "" : copyLive ? "⇣ " + copyPercent + "%" : "⇣ copying"
    // Drop shelf (main.qml: shelfEnabled = online && the setting). A file drag over the notch
    // lights the outline in the accent (one change on enter, one on leave); a drop holds it
    // ~600 ms, amber when nothing could be parked, then one step back. No animation, no timer
    // while idle: zero redraws until a file drag actually passes over this notch.
    property bool shelfEnabled: false
    signal filesDropped(var urls)
    property bool dropHover: false
    property int dropFlash: 0           // 1 parked, -1 refused, 0 none
    readonly property bool dropShown: dropHover || dropFlash !== 0
    readonly property color dropColor: dropFlash < 0 ? alertColor : accent
    function shelfResult(count) { dropFlash = count > 0 ? 1 : -1; dropFlashHold.restart(); }
    onShelfEnabledChanged: if (!shelfEnabled) { dropHover = false; dropFlash = 0; }
    Timer { id: dropFlashHold; objectName: "dropFlashHold"; interval: 600; onTriggered: pill.dropFlash = 0 }
    readonly property bool motion: Kirigami.Units.longDuration > 0
    // Every notch repaint costs Plasma and KWin, on every monitor. The working pulse and the
    // music bars share one stepped clock (~8 updates/s, whole-pixel bar heights) instead of
    // frame-rate animations, and with the companion on they move only on the monitor the
    // pointer is on; elsewhere they hold a still frame that still shows the state.
    readonly property bool animateHere: !companion || pointerAt <= 0 || companionFace.attended
    property int frame: 0
    Timer {
        id: stepper
        objectName: "notchStepper"
        interval: 125
        repeat: true
        running: (pill.pulsing || pill.playing || (pill.companionShown && companionFace.clockWanted)) && pill.visible && pill.motion && pill.animateHere && !pill.companionFrozen
        onTriggered: pill.frame = (pill.frame + 1) % 7560
    }
    // 0..1 breath on the same clock: working 2.25 s in 18 steps; waiting a slower 4.5 s in
    // the same 18 steps, one per two ticks, so every other tick changes nothing and draws
    // nothing. Steady at .6 when not animating.
    readonly property real workPulse: !pulsing ? 0 : !(stepper.running || companionFrozen) ? .6
        : (1 - Math.cos((waitingShown ? Math.floor(frame / 2) : frame) * Math.PI / 9)) / 2
    readonly property bool hovered: pointer.containsMouse
    property real hoverProgress: hovered || activeFocus ? 1 : 0
    signal activated()
    clip: true
    activeFocusOnTab: true
    opacity: stealth && !hovered && !activeFocus ? 0.15 : 1
    Accessible.role: Accessible.Button
    Accessible.name: "Hermes Island"
    Accessible.description: "Click to open or minimize Hermes Island"
        + (waitingShown ? ". Hermes is waiting for you" : working ? ". Hermes is working" : "")
        + (timerShown ? ". Timer " + timerText + (timerEndsAt > 0 ? "" : ", paused") : "")
        + (micMuted ? ". Microphone muted" : "")
        + (copyShown ? ". Copying files, " + copyPercent + " percent" : "")
        + (shelfEnabled ? ". Drop files here to park them on the shelf" : "")
    Accessible.onPressAction: activated()
    Keys.onSpacePressed: event => { if (!event.isAutoRepeat) activated(); event.accepted = true; }
    Keys.onReturnPressed: event => { if (!event.isAutoRepeat) activated(); event.accepted = true; }
    Behavior on hoverProgress { NumberAnimation { duration: Kirigami.Units.longDuration; easing.type: Easing.OutCubic } }
    Behavior on opacity { NumberAnimation { duration: Kirigami.Units.longDuration } }
    // Ticks only while a countdown is actually running and the notch is on screen.
    Timer {
        interval: 1000 - Date.now() % 1000 + 5
        running: pill.timerEndsAt > 0 && pill.visible
        repeat: true
        triggeredOnStart: true
        onTriggered: { pill.nowMs = Date.now(); interval = 1000 - pill.nowMs % 1000 + 5; }
    }
    Shape {
        id: surface
        objectName: "notchSurface"
        anchors.horizontalCenter: parent.horizontalCenter
        // Hover tints the fill; fixed silhouette keeps every close endpoint exact.
        width: pill.width
        height: pill.height
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            id: outline
            objectName: "notchOutline"
            strokeWidth: pill.dropShown ? 1.5 : pill.pulsing ? Math.max(1, pill.borderEnabled ? Math.min(4, pill.borderWidth) : 0) : pill.borderEnabled ? Math.max(0, Math.min(4, pill.borderWidth)) : 0
            strokeColor: pill.dropShown ? pill.dropColor : pill.pulsing
                ? Qt.rgba(pill.pulseColor.r, pill.pulseColor.g, pill.pulseColor.b, (pill.waitingShown ? .35 : .25) + .55 * pill.workPulse)
                : Qt.rgba(pill.borderColor.r, pill.borderColor.g, pill.borderColor.b, Math.max(0, Math.min(1, pill.borderOpacity)))
            fillColor: pill.pulsing && .05 + .09 * pill.workPulse > .09 * pill.hoverProgress
                ? Qt.tint(pill.background, Qt.rgba(pill.pulseColor.r, pill.pulseColor.g, pill.pulseColor.b, .05 + .09 * pill.workPulse))
                : Qt.tint(pill.background, Qt.rgba(pill.accent.r, pill.accent.g, pill.accent.b, .09 * pill.hoverProgress))
            // Flat crown, concave shoulders and rounded lower corners: not a capsule.
            PathSvg { path: {
                const w = surface.width, h = surface.height;
                const s = Math.min(6, h / 4), r = Math.min(10, h / 2 - 1);
                const i = Math.max(.5, outline.strokeWidth / 2);
                return "M " + i + " " + i + " H " + (w - i)
                    + " C " + (w - s) + " " + i + " " + (w - s) + " " + (s / 2) + " " + (w - s) + " " + s
                    + " V " + (h - r - i) + " Q " + (w - s) + " " + (h - i) + " " + (w - s - r) + " " + (h - i)
                    + " H " + (s + r) + " Q " + s + " " + (h - i) + " " + s + " " + (h - r - i)
                    + " V " + s + " C " + s + " " + (s / 2) + " " + s + " " + i + " " + i + " " + i + " Z";
            } }
        }
    }
    // Hover peek. The closed native notch never opens the island's hover preview (its input
    // region is empty in Still mode and the widget sends no enter/leave), so the two can't
    // collide; the peek waits the same `hoverDelay` the preview uses (180 ms when that is
    // 0 = preview off) so a pointer passing over the panel doesn't flash it.
    readonly property string peekText: !hoverPeek ? ""
        : playing && peekTrack !== "" ? peekTrack
        : timerShown ? (timerEndsAt > 0 ? "Timer " + timerText + " left" : "Timer paused, " + timerText) : ""
    property bool peekReady: false
    readonly property bool peeking: peekReady && hovered && peekText !== ""
    onHoveredChanged: { peekReady = false; if (hovered && hoverPeek) peekDelay.restart(); else peekDelay.stop(); }
    Timer { id: peekDelay; objectName: "peekDelay"; interval: pill.hoverDelay > 0 ? pill.hoverDelay : 180; onTriggered: pill.peekReady = true }
    Text {
        id: peekLine
        objectName: "notchPeek"
        // Between the companion/mic marks and the music bars; one line, ellipsized.
        readonly property real from: micMutedMark.opacity > 0 ? micMutedMark.x + micMutedMark.width + 4
            : pill.companionShown ? companionFace.x + companionFace.width + 4 : 14
        readonly property real to: pill.playing ? musicIndicator.x - 4 : pill.width - 14
        x: from
        width: Math.max(0, to - from)
        anchors.verticalCenter: parent.verticalCenter
        text: pill.peekText
        elide: Text.ElideRight
        maximumLineCount: 1
        horizontalAlignment: Text.AlignHCenter
        color: pill.playing && pill.peekTrack !== "" ? pill.textColor : pill.accent
        font.pixelSize: 11
        font.weight: Font.DemiBold
        font.features: { "tnum": 1 }
        visible: opacity > 0
        opacity: pill.peeking ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
    }
    Column {
        anchors.centerIn: parent
        // A detailed companion adds up to 6 px before the mic mark. Give the clock
        // the same extra clearance; default 20 px companions retain their layout.
        anchors.horizontalCenterOffset: pill.companionShown ? Math.max(0, companionFace.preferredSize - 20) : 0
        spacing: -1
        opacity: pill.peeking ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        readonly property bool twoLines: pill.showDate || pill.timerShown || pill.copyShown
        Text {
            objectName: "panelClock"
            anchors.horizontalCenter: parent.horizontalCenter
            text: pill.clockText
            color: pill.textColor
            font.pixelSize: parent.twoLines ? 12 : 14
            font.weight: Font.Bold
            height: parent.twoLines ? 14 : 18
            verticalAlignment: Text.AlignVCenter
        }
        Text {
            objectName: "panelDate"
            anchors.horizontalCenter: parent.horizontalCenter
            // A running or paused timer takes the date's line, then a file copy.
            text: pill.timerShown ? (pill.timerEndsAt > 0 ? "" : "❚❚ ") + pill.timerText
                : pill.copyText !== "" ? pill.copyText : pill.dateText
            visible: parent.twoLines
            font.features: { "tnum": 1 }
            color: pill.timerShown || pill.copyShown ? pill.accent : pill.textColor
            font.pixelSize: 10
            font.weight: Font.DemiBold
            height: 12
            verticalAlignment: Text.AlignVCenter
        }
    }
    Row {
        id: musicIndicator
        objectName: "notchMusicIndicator"
        x: pill.width - 29
        anchors.verticalCenter: parent.verticalCenter
        width: 14
        height: 12
        spacing: 2
        opacity: pill.playing ? 1 : 0
        readonly property bool animating: pill.playing && (stepper.running || pill.companionFrozen)
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        Repeater {
            model: 4
            Rectangle {
                id: bar
                required property int index
                // Each bar bounces between its own low and high in whole pixels, on its own
                // period, so the four never move in lockstep.
                readonly property int low: 3 + index % 3
                readonly property int high: 11 - index % 2 * 3
                readonly property int period: 5 + index * 2
                readonly property int phase: ((musicIndicator.animating ? pill.frame : 0) + index * 3) % (2 * period)
                readonly property int level: low + Math.round((high - low) * (phase < period ? phase : 2 * period - phase) / period)
                objectName: "notchMusicBar" + index
                width: 2
                height: pill.playing ? level : 2
                y: Math.floor((musicIndicator.height - height) / 2)
                radius: 1
                color: pill.textColor
            }
        }
    }
    Companion {
        id: companionFace
        objectName: "notchCompanion"
        x: 9
        anchors.verticalCenter: parent.verticalCenter
        character: pill.companionCharacter
        width: preferredSize
        height: preferredSize
        visible: opacity > 0
        opacity: pill.companionShown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        pointerX: pill.pointerX
        pointerY: pill.pointerY
        pointerAt: pill.pointerAt
        frozen: pill.companionFrozen
        frame: pill.frame
        happy: pill.hovered
        reactive: pill.companionReactive
        working: pill.working && pill.companionReactive
        waiting: pill.waiting && pill.companionReactive
        alertColor: pill.alertColor
        motion: pill.motion
        bodyColor: pill.textColor
        eyeColor: pill.background
    }
    Kirigami.Icon {
        id: micMutedMark
        objectName: "notchMicMuted"
        source: "microphone-sensitivity-muted-symbolic"
        x: pill.companionShown ? companionFace.x + companionFace.width + 4 : 15
        anchors.verticalCenter: parent.verticalCenter
        width: 12
        height: 12
        color: pill.textColor
        isMask: true
        visible: opacity > 0
        opacity: pill.micMuted ? .85 : 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
    }
    DropArea {
        id: shelfDrop
        objectName: "notchDropArea"
        anchors.fill: parent
        enabled: pill.shelfEnabled
        // Only local files, and only as a copy or a link: never a move, which would make a
        // file manager delete the original after the drop.
        function wanted(drag) {
            return Shelf.fileUrls(drag.urls).length > 0
                && (drag.supportedActions & (Qt.CopyAction | Qt.LinkAction)) !== 0;
        }
        function action(drag) { return (drag.supportedActions & Qt.CopyAction) ? Qt.CopyAction : Qt.LinkAction; }
        onEntered: drag => {
            if (!wanted(drag)) { drag.accepted = false; return; }
            drag.accept(action(drag));
            pill.dropHover = true;
        }
        onExited: pill.dropHover = false
        onDropped: drop => {
            pill.dropHover = false;
            if (!wanted(drop)) { drop.accepted = false; return; }
            drop.accept(action(drop));
            pill.filesDropped(Shelf.fileUrls(drop.urls));
        }
    }
    MouseArea {
        id: pointer
        // The indicator never takes input; the original full-slot click target is retained.
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        // Fire on press, like docks and panels do: saves the whole press-to-release time.
        onPressed: mouse => { if (mouse.button === Qt.LeftButton) pill.activated(); }
    }
}
