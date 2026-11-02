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
    readonly property bool hovered: pointer.containsMouse
    property real hoverProgress: hovered || activeFocus ? 1 : 0
    signal activated()
    clip: true
    activeFocusOnTab: true
    opacity: stealth && !hovered && !activeFocus ? 0.15 : 1
    Accessible.role: Accessible.Button
    Accessible.name: "Hermes Island"
    Accessible.description: "Click to open or minimize Hermes Island"
    Accessible.onPressAction: activated()
    Keys.onSpacePressed: event => { if (!event.isAutoRepeat) activated(); event.accepted = true; }
    Keys.onReturnPressed: event => { if (!event.isAutoRepeat) activated(); event.accepted = true; }
    Behavior on hoverProgress { NumberAnimation { duration: Kirigami.Units.longDuration; easing.type: Easing.OutCubic } }
    Behavior on opacity { NumberAnimation { duration: Kirigami.Units.longDuration } }
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
            strokeWidth: pill.borderEnabled ? Math.max(0, Math.min(4, pill.borderWidth)) : 0
            strokeColor: Qt.rgba(pill.borderColor.r, pill.borderColor.g, pill.borderColor.b, Math.max(0, Math.min(1, pill.borderOpacity)))
            fillColor: Qt.tint(pill.background, Qt.rgba(pill.accent.r, pill.accent.g, pill.accent.b, .09 * pill.hoverProgress))
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
        Text {
            objectName: "panelClock"
            anchors.horizontalCenter: parent.horizontalCenter
            text: pill.clockText
            color: pill.textColor
            font.pixelSize: pill.showDate ? 12 : 14
            font.weight: Font.Bold
            height: pill.showDate ? 14 : 18
            verticalAlignment: Text.AlignVCenter
        }
        Text {
            objectName: "panelDate"
            anchors.horizontalCenter: parent.horizontalCenter
            text: pill.dateText
            visible: pill.showDate
            color: pill.textColor
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
        readonly property bool animating: pill.playing && pill.visible && Kirigami.Units.longDuration > 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        Repeater {
            model: 4
            Rectangle {
                id: bar
                required property int index
                property real level: 3
                objectName: "notchMusicBar" + index
                width: 2
                height: pill.playing ? level : 2
                y: (musicIndicator.height - height) / 2
                radius: 1
                color: pill.textColor
                SequentialAnimation on level {
                    running: musicIndicator.animating
                    loops: Animation.Infinite
                    NumberAnimation { to: 11 - bar.index % 2 * 3; duration: 270 + bar.index * 83; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 3 + bar.index % 3; duration: 360 + bar.index * 57; easing.type: Easing.InOutSine }
                }
            }
        }
    }
    MouseArea {
        id: pointer
        // The indicator never takes input; the original full-slot click target is retained.
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: pill.activated()
    }
}
