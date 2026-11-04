import QtQuick
import QtQuick.Shapes
import org.kde.kirigami as Kirigami

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
    property bool micMuted: false
    // Notch companion: a small face at the left whose eyes follow the global pointer.
    property bool companion: false
    property string companionCharacter: "blob"
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    // On a narrow notch the mic mark needs the companion's room.
    readonly property bool companionShown: companion && !(micMuted && width < 140)
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
        running: (pill.working || pill.playing) && pill.visible && pill.motion && pill.animateHere
        onTriggered: pill.frame = (pill.frame + 1) % 7560
    }
    // 0..1 accent breath, a 2.25 s cycle in 18 steps; steady at .6 when not animating.
    readonly property real workPulse: !working ? 0 : stepper.running ? (1 - Math.cos(frame * Math.PI / 9)) / 2 : .6
    readonly property bool hovered: pointer.containsMouse
    property real hoverProgress: hovered || activeFocus ? 1 : 0
    signal activated()
    clip: true
    activeFocusOnTab: true
    opacity: stealth && !hovered && !activeFocus ? 0.15 : 1
    Accessible.role: Accessible.Button
    Accessible.name: "Hermes Island"
    Accessible.description: "Click to open or minimize Hermes Island"
        + (working ? ". Hermes is working" : "")
        + (timerShown ? ". Timer " + timerText + (timerEndsAt > 0 ? "" : ", paused") : "")
        + (micMuted ? ". Microphone muted" : "")
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
            strokeWidth: pill.working ? Math.max(1, pill.borderEnabled ? Math.min(4, pill.borderWidth) : 0) : pill.borderEnabled ? Math.max(0, Math.min(4, pill.borderWidth)) : 0
            strokeColor: pill.working
                ? Qt.rgba(pill.accent.r, pill.accent.g, pill.accent.b, .25 + .55 * pill.workPulse)
                : Qt.rgba(pill.borderColor.r, pill.borderColor.g, pill.borderColor.b, Math.max(0, Math.min(1, pill.borderOpacity)))
            fillColor: Qt.tint(pill.background, Qt.rgba(pill.accent.r, pill.accent.g, pill.accent.b, Math.max(.09 * pill.hoverProgress, pill.working ? .05 + .09 * pill.workPulse : 0)))
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
    Column {
        anchors.centerIn: parent
        spacing: -1
        readonly property bool twoLines: pill.showDate || pill.timerShown
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
            // A running or paused timer takes the date's line.
            text: pill.timerShown ? (pill.timerEndsAt > 0 ? "" : "❚❚ ") + pill.timerText : pill.dateText
            visible: parent.twoLines
            font.features: { "tnum": 1 }
            color: pill.timerShown ? pill.accent : pill.textColor
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
        readonly property bool animating: pill.playing && stepper.running
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
        happy: pill.hovered
        working: pill.working
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
