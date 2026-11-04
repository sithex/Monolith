import QtQuick
import QtQuick.Shapes

// Isaac, after The Binding of Isaac: a big pale head, two large dark eyes with a glint,
// a small frown, and now and then a tear rolling down while the pointer rests.
// Hovering the notch cheers him up for a moment. Drawn from `face`, the brain in
// ../Companion.qml; his skin keeps its own colours whatever the notch's scheme.
Item {
    id: art
    property Item face
    readonly property real preferredSize: 18
    // His eyes are large, so they travel a little less to stay inside his head.
    readonly property real gazeScale: .8
    readonly property real size: face ? face.size : 0
    readonly property color skin: "#f2d0c0"
    readonly property color outline: "#5a3a33"
    readonly property color eye: "#1b1113"

    // -1 frown … 1 smile; changes only on hover, so the mouth re-tessellates rarely.
    property real mood: face && face.happy ? 1 : -1
    Behavior on mood { enabled: art.face && art.face.motion; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

    Connections {
        target: art.face
        function onBlinked(resting) {
            if (resting && !art.face.happy && !art.face.thinking && art.face.motion && Math.random() < .35)
                art.cry(Math.random() < .5 ? 0 : 1);
        }
    }

    Item {
        id: head
        width: art.size
        height: art.size
        anchors.centerIn: parent
        transform: Translate { x: art.face.quarter(art.face.shownX * .25); y: art.face.quarter(art.face.shownY * .2) }

        Rectangle {
            objectName: "companionBody"
            x: art.size * .05
            y: art.size * .05
            width: art.size * .9
            height: art.size * .88
            radius: width / 2
            color: art.skin
            border.color: art.outline
            border.width: Math.max(.75, art.size * .045)
            antialiasing: true
        }

        Repeater {
            model: [.33, .67]
            Rectangle {
                required property real modelData
                required property int index
                objectName: "companionEye" + index
                readonly property real eyeH: art.size * .26 * art.face.lid * art.face.blink
                width: art.size * .23
                height: Math.max(1, eyeH)
                radius: Math.min(width, height) / 2
                x: art.size * modelData - width / 2 + art.face.shownX
                y: art.size * .5 - height / 2 + art.face.shownY + (1 - art.face.lid) * art.size * .08
                color: art.eye
                antialiasing: true
                // The glint that makes them look wet; gone when the eyes close.
                Rectangle {
                    width: art.size * .09
                    height: width
                    radius: width / 2
                    x: parent.width * .2
                    y: parent.height * .16
                    color: "white"
                    visible: parent.height > art.size * .12
                    antialiasing: true
                }
            }
        }

        Shape {
            objectName: "isaacMouth"
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: art.outline
                strokeWidth: Math.max(.9, art.size * .06)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                startX: art.size * .41
                startY: art.size * .8
                PathQuad { x: art.size * .59; y: art.size * .8; controlX: art.size * .5; controlY: art.size * (.8 + .08 * art.mood) }
            }
        }

        // A tear from under one eye: a short one-off animation, nothing between them.
        Rectangle {
            objectName: "isaacTear"
            width: art.size * .11
            height: art.size * .15
            radius: width / 2
            color: "#9fd4ff"
            border.color: "#4a8fd0"
            border.width: Math.max(.5, art.size * .025)
            antialiasing: true
            visible: tearFall.running
            x: art.size * (art.tearEye === 0 ? .33 : .67) - width / 2 + art.face.shownX * .5
            y: art.size * .6 + art.drop * art.size * .32
            opacity: 1 - art.drop * art.drop
        }
    }

    property int tearEye: 0
    property real drop: 0
    function cry(eye) {
        if (tearFall.running) return;
        tearEye = eye;
        tearFall.start();
    }
    NumberAnimation { id: tearFall; target: art; property: "drop"; from: 0; to: 1; duration: 900; easing.type: Easing.InQuad }
}
