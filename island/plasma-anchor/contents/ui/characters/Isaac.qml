import QtQuick
import QtQuick.Shapes

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
    Behavior on cheer { enabled: art.face && art.face.motion; NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }

    Connections {
        target: art.face
        function onBlinked(resting) {
            if (resting && !art.face.happy && !art.face.thinking && !art.face.sleepy && art.face.motion && Math.random() < .3)
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
            model: [.25, .75]
            Rectangle {
                required property real modelData
                objectName: "isaacStream"
                width: art.size * .15
                height: art.size * .25
                radius: width / 2
                x: art.size * modelData - width / 2 + (modelData < .5 ? art.size * .01 : -art.size * .01)
                y: art.size * .6
                color: art.tearColor
                border.color: art.tearEdge
                border.width: Math.max(.4, art.size * .02)
                antialiasing: true
                opacity: 1 - art.cheer * .9
                visible: opacity > .02
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
