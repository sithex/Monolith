import QtQuick
import QtQuick.Shapes

// The notch companion: a small round face whose eyes follow the pointer anywhere on
// screen. The global pointer arrives as a D-Bus signal (see main.qml), already throttled
// at the source and sent only while the pointer moves; everything here is event-driven
// except a blink timer that runs only while the face is shown.
Item {
    id: face
    // Global pointer position in logical px and when it last moved (ms, 0 = never).
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    property bool happy: false      // pointer over the notch
    property bool working: false    // Hermes is busy: a thinking glance when the pointer rests
    property bool motion: true      // false when the desktop disables animations
    property color bodyColor: "#f1ede4"
    property color eyeColor: "#171717"
    readonly property real size: Math.min(width, height)

    // Eye offset in shares of the face size: toward the pointer, more the farther it is,
    // always inside the face.
    property real lookX: 0
    property real lookY: 0
    property real blink: 1          // vertical eye scale; dips to ~0 for a blink
    property bool sleepy: false
    // How open the eyes are when not blinking: heavy-lidded while dozing.
    property real lid: sleepy ? .42 : 1
    Behavior on lid { enabled: face.motion; NumberAnimation { duration: 600; easing.type: Easing.InOutSine } }
    property bool thinking: false
    property real glance: 0         // a side look now and then while nothing moves

    readonly property real maxX: .13
    readonly property real maxY: .09

    function updateLook() {
        if (pointerAt <= 0) return;
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        const dx = pointerX - centre.x, dy = pointerY - centre.y;
        // Soft saturation: near the notch the eyes track closely, across the screen they
        // settle at the edge of the face instead of snapping there.
        lookX = maxX * dx / (Math.abs(dx) + 160);
        lookY = maxY * dy / (Math.abs(dy) + 90);
    }
    onPointerAtChanged: {
        updateLook();
        if (sleepy) wake.restart();
        sleepy = false;
        drowsy.restart();
        thinking = false;
        if (working) ponder.restart();
    }
    onWorkingChanged: { if (working) ponder.restart(); else { ponder.stop(); thinking = false; } }
    onVisibleChanged: if (visible) updateLook()

    Behavior on lookX { enabled: face.motion; SmoothedAnimation { velocity: .9; easing.type: Easing.OutCubic } }
    Behavior on lookY { enabled: face.motion; SmoothedAnimation { velocity: .9; easing.type: Easing.OutCubic } }
    Behavior on glance { enabled: face.motion; NumberAnimation { duration: 260; easing.type: Easing.InOutSine } }

    // Where the eyes actually point: the pointer, a thinking look, or a passing glance.
    readonly property real gazeX: thinking ? .08 : sleepy ? 0 : lookX + glance
    readonly property real gazeY: thinking ? -.07 : sleepy ? .05 : lookY
    property real shownX: gazeX
    property real shownY: gazeY
    Behavior on shownX { enabled: face.motion; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    Behavior on shownY { enabled: face.motion; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

    // Dozes off after three quiet minutes, wakes with a wide-eyed blink.
    Timer { id: drowsy; interval: 180000; running: face.visible && !face.working; onTriggered: face.sleepy = true }
    Timer { id: ponder; interval: 1500; onTriggered: face.thinking = face.working }
    SequentialAnimation {
        id: wake
        NumberAnimation { target: face; property: "blink"; to: 1.25; duration: 120; easing.type: Easing.OutQuad }
        NumberAnimation { target: face; property: "blink"; to: 1; duration: 220; easing.type: Easing.InOutQuad }
    }
    SequentialAnimation {
        id: blinkOnce
        NumberAnimation { target: face; property: "blink"; to: .08; duration: 70; easing.type: Easing.InQuad }
        PauseAnimation { duration: 50 }
        NumberAnimation { target: face; property: "blink"; to: 1; duration: 130; easing.type: Easing.OutQuad }
    }
    Timer {
        id: blinker
        interval: 3000
        running: face.visible && face.motion && !face.sleepy && !face.happy
        onTriggered: {
            blinkOnce.restart();
            // Now and then, with the pointer resting, the eyes wander to one side and back.
            if (Date.now() - face.pointerAt > 4000 && !face.thinking && Math.random() < .25) {
                face.glance = (Math.random() < .5 ? -1 : 1) * .09;
                unglance.restart();
            }
            interval = 2600 + Math.random() * 3800;
            restart();
        }
    }
    Timer { id: unglance; interval: 1300; onTriggered: face.glance = 0 }

    Item {
        id: head
        width: face.size
        height: face.size
        anchors.centerIn: parent
        // The whole head leans a little toward the pointer, the eyes a lot more.
        transform: Translate { x: face.shownX * face.size * .25; y: face.shownY * face.size * .2 }

        Rectangle {
            id: body
            objectName: "companionBody"
            x: face.size * .06
            y: face.size * .06
            width: face.size * .88
            height: face.size * .88
            radius: width / 2
            color: face.bodyColor
            antialiasing: true
        }

        Repeater {
            model: [.36, .64]
            Rectangle {
                required property real modelData
                required property int index
                objectName: "companionEye" + index
                readonly property real eyeH: face.size * .24 * face.lid * face.blink
                width: face.size * .12
                height: Math.max(1, eyeH)
                radius: width / 2
                x: face.size * modelData - width / 2 + face.shownX * face.size
                y: face.size * .52 - height / 2 + face.shownY * face.size + (1 - face.lid) * face.size * .1
                color: face.eyeColor
                antialiasing: true
                opacity: face.happy ? 0 : 1
                Behavior on opacity { NumberAnimation { duration: 140 } }
            }
        }

        // Pleased: the eyes turn into little arches while the pointer is on the notch.
        Shape {
            anchors.fill: parent
            opacity: face.happy ? 1 : 0
            visible: opacity > 0
            preferredRendererType: Shape.CurveRenderer
            Behavior on opacity { NumberAnimation { duration: 140 } }
            ShapePath {
                id: leftArch
                strokeColor: face.eyeColor
                strokeWidth: Math.max(1.2, face.size * .1)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                readonly property real cx: face.size * .36 + face.shownX * face.size * .5
                readonly property real cy: face.size * .55
                readonly property real span: face.size * .085
                startX: cx - span
                startY: cy
                PathQuad { x: leftArch.cx + leftArch.span; y: leftArch.cy; controlX: leftArch.cx; controlY: leftArch.cy - face.size * .2 }
            }
            ShapePath {
                id: rightArch
                strokeColor: face.eyeColor
                strokeWidth: Math.max(1.2, face.size * .1)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                readonly property real cx: face.size * .64 + face.shownX * face.size * .5
                readonly property real cy: face.size * .55
                readonly property real span: face.size * .085
                startX: cx - span
                startY: cy
                PathQuad { x: rightArch.cx + rightArch.span; y: rightArch.cy; controlX: rightArch.cx; controlY: rightArch.cy - face.size * .2 }
            }
        }
    }
}
