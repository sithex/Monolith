import QtQuick
import QtQuick.Shapes
import QtQuick.Window

// Isaac, after The Binding of Isaac: a round, slightly wide head with a thick dark
// outline, big black eyes set wide apart with a white glint, a small open wailing mouth
// and tears always streaming down both cheeks. Hovering the notch cheers him up: the
// tears dry and the mouth closes into a little smile.
//
// Idle habit: with the pointer resting, he now and then shoots a tear the way he looks
// (his attack in the game); it arcs a little and splashes. A one-off of under half a
// second; between shots nothing animates. Drawn from `face`, the brain in
// ../Companion.qml; his colours are his own whatever the notch's scheme.
Item {
    id: art
    property Item face
    readonly property real preferredSize: 18
    // His eyes sit near the sides of his head, so they travel less.
    readonly property real gazeScale: .6
    readonly property real size: face ? face.size : 0
    readonly property color skin: "#e3c6c5"
    readonly property color rim: "#cf9c9b"
    readonly property color outline: "#140606"
    readonly property color tearColor: "#9cc8cc"
    readonly property color tearEdge: "#68989c"

    // 1 while the pointer is on the notch: tears dry up, the mouth smiles.
    property real cheer: face && face.happy ? 1 : 0
    Behavior on cheer { enabled: art.face && art.face.motion && !art.face.frozen; NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }

    Connections {
        target: art.face
        // Game-mode freeze: a tear in flight or a drip is dropped, not left hanging mid-air.
        function onFrozenChanged() {
            if (!art.face.frozen) return;
            shotAnim.stop(); dripAnim.stop();
            art.shot = 0; art.splash = 0; art.dripAt = 0;
        }
        function onBlinked(resting) {
            if (resting && art.face.attended && !art.face.frozen && !art.face.happy && !art.face.thinking && !art.face.sleepy && art.face.motion && Math.random() < .3)
                art.shoot();
        }
    }

    Item {
        id: head
        width: art.size
        height: art.size
        anchors.centerIn: parent
        transform: Translate { x: art.face.quarter(art.face.shownX * .25); y: art.face.quarter(art.face.shownY * .2) }

        // Head: outline, then a darker rim of skin around the lighter face.
        Rectangle {
            objectName: "companionBody"
            x: 0
            y: art.size * .03
            width: art.size
            height: art.size * .92
            radius: height / 2
            color: art.outline
            antialiasing: true
            Rectangle {
                anchors.fill: parent
                anchors.margins: Math.max(1, art.size * .08)
                radius: height / 2
                color: art.rim
                antialiasing: true
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: Math.max(.5, art.size * .035)
                    anchors.topMargin: Math.max(.5, art.size * .05)
                    radius: height / 2
                    color: art.skin
                    antialiasing: true
                }
            }
        }

        // Tears streaming from under each eye down his cheeks, always (dry on hover).
        Repeater {
            id: streams
            model: [.25, .75]
            Rectangle {
                required property real modelData
                objectName: "isaacStream"
                width: art.size * .17
                height: art.size * .3
                radius: width / 2
                // Hangs from the eye wherever it looks, so turning his head keeps them joined.
                x: art.size * modelData - width / 2 + (modelData < .5 ? art.size * .01 : -art.size * .01) + art.face.shownX * .7
                y: art.size * .6 + Math.min(0, art.face.shownY)
                color: art.tearColor
                border.color: art.tearEdge
                border.width: Math.max(.4, art.size * .02)
                antialiasing: true
                opacity: 1 - art.cheer * .9
                visible: opacity > .02
                // Flow: light bands slide down the stream a whole screen pixel at a time,
                // like the game's pixel art; a tick that wouldn't move them a pixel draws
                // nothing. A sub-pixel band just smears into the stream at 100% scale.
                Item {
                    anchors.fill: parent
                    anchors.topMargin: parent.border.width
                    clip: true
                    readonly property real px: 1 / Math.max(1, Screen.devicePixelRatio)
                    Repeater {
                        model: 2
                        Rectangle {
                            required property int index
                            objectName: "isaacFlow"
                            readonly property real period: parent.height
                            readonly property real px: parent.px
                            width: parent.width
                            height: Math.max(px, Math.round(art.size * .06 / px) * px)
                            x: 0
                            y: Math.round(((art.flowFrame / art.flowSteps + index / 2) % 1) * period / px) * px
                            color: "#f2fbfc"
                        }
                    }
                }
            }
        }

        Repeater {
            model: [.25, .75]
            Rectangle {
                required property real modelData
                required property int index
                objectName: "companionEye" + index
                readonly property real eyeH: art.size * .27 * art.face.lid * art.face.blink
                width: art.size * .25
                height: Math.max(1, eyeH)
                radius: Math.min(width, height) * .42
                x: art.size * modelData - width / 2 + art.face.shownX
                y: art.size * .52 - height / 2 + art.face.shownY + (1 - art.face.lid) * art.size * .07
                color: art.outline
                antialiasing: true
                // The glint, up and to the left as in the game; gone when the eyes close.
                Rectangle {
                    width: art.size * .085
                    height: width
                    radius: width * .3
                    x: parent.width * .2
                    y: parent.height * .18
                    color: "white"
                    visible: parent.height > art.size * .12
                    antialiasing: true
                }
            }
        }

        // The wailing mouth: small, open, a row of teeth along the bottom.
        Rectangle {
            objectName: "isaacMouth"
            width: art.size * .22
            height: art.size * .16
            radius: art.size * .05
            x: art.size * .5 - width / 2
            y: art.size * .64
            color: art.outline
            antialiasing: true
            opacity: 1 - art.cheer
            visible: opacity > .02
            Rectangle {
                width: parent.width * .62
                height: Math.max(.6, parent.height * .28)
                x: (parent.width - width) / 2
                y: parent.height * .5
                color: "white"
                radius: height / 2
            }
        }
        // Cheered up: a little smile instead.
        Shape {
            objectName: "isaacSmile"
            anchors.fill: parent
            opacity: art.cheer
            visible: opacity > .02
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: art.outline
                strokeWidth: Math.max(.9, art.size * .065)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                startX: art.size * .4
                startY: art.size * .68
                PathQuad { x: art.size * .6; y: art.size * .68; controlX: art.size * .5; controlY: art.size * .79 }
            }
        }
    }

    // Choppy pixel-style flow: the streams' bands step down `flowSteps` frames per cycle
    // at 8 ticks a second, both streams on the same tick (one panel redraw per tick).
    // Only on the monitor the pointer is on; stops while cheered up, dozing, settled (no
    // pointer move for a minute: the streams hold a still frame) or with animations off.
    readonly property int flowSteps: 8
    property int flowFrame: 0
    // Asks the notch for its stepped clock (the same ~8 Hz tick that drives the working pulse
    // and the music bars) instead of running a timer of its own: two timers of equal period
    // fire out of phase and repaint the panel twice per tick.
    readonly property bool clockWanted: flowing && !!face && face.attended && face.visible && face.motion && !face.happy && !face.sleepy && !face.settled && !face.frozen
    Connections {
        target: art.face
        function onFrameChanged() { if (art.clockWanted) art.flowFrame = (art.flowFrame + 1) % art.flowSteps; }
    }
    // Flow and drip are alternatives; flow is on.
    readonly property bool flowing: true

    // A drip every few seconds: a bead runs down one stream and drops off his cheek.
    // About 0.9 s of animation, then nothing until the next; none while cheered up,
    // dozing or with animations turned off.
    property int dripEye: 0
    property real dripAt: 0         // 0 … 1 down the stream, then off it
    // What's drawn moves in 12 steps (about half a pixel each): the animation ticks every
    // frame, but the bead only changes, and the panel only redraws, 12 times a drip.
    readonly property real dripStep: Math.round(dripAt * 12) / 12
    Timer {
        id: dripper
        interval: 3500
        running: !art.flowing && art.face && art.face.attended && art.face.visible && art.face.motion && !art.face.happy && !art.face.sleepy && !art.face.frozen
        repeat: true
        onTriggered: {
            if (!dripAnim.running) {
                art.dripEye = Math.random() < .5 ? 0 : 1;
                dripAnim.start();
            }
            interval = 3500 + Math.random() * 2500;
        }
    }
    Rectangle {
        objectName: "isaacDrip"
        parent: head
        readonly property Item stream: streams.itemAt(art.dripEye)
        width: art.size * .13
        height: art.size * (.12 + .05 * Math.max(0, art.dripStep - .7) / .3)
        radius: width / 2
        color: art.tearColor
        border.color: art.tearEdge
        border.width: Math.max(.4, art.size * .02)
        antialiasing: true
        visible: dripAnim.running && stream !== null
        x: stream ? stream.x + (stream.width - width) / 2 : 0
        y: stream ? stream.y + art.dripStep * (stream.height + art.size * .1) - height / 2 : 0
        opacity: art.dripStep < .75 ? 1 : 1 - (art.dripStep - .75) / .25
        Rectangle {
            width: parent.width * .35
            height: width
            radius: width / 2
            x: parent.width * .22
            y: parent.height * .2
            color: "white"
            opacity: .7
        }
    }
    NumberAnimation { id: dripAnim; target: art; property: "dripAt"; from: 0; to: 1; duration: 900; easing.type: Easing.InQuad }

    // The tear shot: from the eye on the side he's looking, a short arc, then a splash.
    // Right goes into the gap before the clock, left to the notch's rounded edge.
    property real shot: 0           // 0 … 1 along the flight
    property real splash: 0         // 0 … 1 of the splash after it lands
    property int shotDir: 1
    function shoot() {
        if (shotAnim.running) return;
        shotDir = face.shownX < 0 ? -1 : 1;
        shotAnim.start();
    }
    readonly property real shotReach: shotDir > 0 ? size * .8 : size * .4
    readonly property real shotStartX: size * (shotDir > 0 ? .75 : .25)
    Rectangle {
        objectName: "isaacShot"
        width: art.size * .22
        height: width
        radius: width / 2
        color: art.tearColor
        border.color: art.tearEdge
        border.width: Math.max(.5, art.size * .03)
        antialiasing: true
        visible: shotAnim.running && art.splash === 0
        x: art.shotStartX + art.shotDir * art.shot * art.shotReach - width / 2
        // Up a touch, then falling: a little gravity, like the game's tears.
        y: art.size * .5 - height / 2 + art.size * (.6 * art.shot * art.shot - .3 * art.shot)
        Rectangle {
            width: parent.width * .28
            height: width
            radius: width / 2
            x: parent.width * .22
            y: parent.height * .2
            color: "white"
            opacity: .7
        }
    }
    Rectangle {
        objectName: "isaacSplash"
        readonly property real landX: art.shotStartX + art.shotDir * art.shotReach
        width: art.size * (.2 + .3 * art.splash)
        height: width * .6
        radius: height / 2
        x: landX - width / 2
        y: art.size * .8 - height / 2
        color: "transparent"
        border.color: art.tearColor
        border.width: Math.max(.6, art.size * .05)
        antialiasing: true
        opacity: 1 - art.splash
        visible: shotAnim.running && art.splash > 0
    }
    SequentialAnimation {
        id: shotAnim
        PropertyAction { target: art; property: "splash"; value: 0 }
        NumberAnimation { target: art; property: "shot"; from: 0; to: 1; duration: 320; easing.type: Easing.Linear }
        NumberAnimation { target: art; property: "splash"; from: 0.01; to: 1; duration: 180; easing.type: Easing.OutQuad }
        PropertyAction { target: art; property: "splash"; value: 0 }
        PropertyAction { target: art; property: "shot"; value: 0 }
    }
}
