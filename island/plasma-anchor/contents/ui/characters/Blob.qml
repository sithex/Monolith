import QtQuick
import QtQuick.Shapes

// The round companion: an ivory ball in the notch's text colour
// with two upright eyes. Draws itself from `face`, the brain in ../Companion.qml.
Item {
    id: art
    property Item face
    readonly property real preferredSize: 16
    readonly property real gazeScale: 1
    readonly property real size: face ? face.size : 0

    Item {
        id: head
        width: art.size
        height: art.size
        anchors.centerIn: parent
        // The whole head leans a little toward the pointer, the eyes a lot more.
        transform: Translate { x: art.face.quarter(art.face.shownX * .25); y: art.face.quarter(art.face.shownY * .2) }

        Rectangle {
            id: body
            objectName: "companionBody"
            x: art.size * .06
            y: art.size * .06
            width: art.size * .88
            height: art.size * .88
            radius: width / 2
            color: art.face.bodyColor
            antialiasing: true
        }

        Repeater {
            model: [.36, .64]
            Rectangle {
                required property real modelData
                required property int index
                objectName: "companionEye" + index
                readonly property real eyeH: art.size * .24 * art.face.lid * art.face.blink
                width: art.size * .12
                height: Math.max(1, eyeH)
                radius: width / 2
                x: art.size * modelData - width / 2 + art.face.shownX
                y: art.size * .52 - height / 2 + art.face.shownY + (1 - art.face.lid) * art.size * .1
                color: art.face.eyeColor
                antialiasing: true
                opacity: art.face.happy ? 0 : 1
                Behavior on opacity { NumberAnimation { duration: 140 } }
            }
        }

        // Pleased: the eyes turn into little arches while the pointer is on the notch.
        Shape {
            anchors.fill: parent
            opacity: art.face.happy ? 1 : 0
            visible: opacity > 0
            preferredRendererType: Shape.CurveRenderer
            Behavior on opacity { NumberAnimation { duration: 140 } }
            ShapePath {
                id: leftArch
                strokeColor: art.face.eyeColor
                strokeWidth: Math.max(1.2, art.size * .1)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                // Follows the gaze only while shown, so hidden arches never re-tessellate.
                readonly property real cx: art.size * .36 + (art.face.happy ? art.face.quarter(art.face.shownX * .5) : 0)
                readonly property real cy: art.size * .55
                readonly property real span: art.size * .085
                startX: cx - span
                startY: cy
                PathQuad { x: leftArch.cx + leftArch.span; y: leftArch.cy; controlX: leftArch.cx; controlY: leftArch.cy - art.size * .2 }
            }
            ShapePath {
                id: rightArch
                strokeColor: art.face.eyeColor
                strokeWidth: Math.max(1.2, art.size * .1)
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                readonly property real cx: art.size * .64 + (art.face.happy ? art.face.quarter(art.face.shownX * .5) : 0)
                readonly property real cy: art.size * .55
                readonly property real span: art.size * .085
                startX: cx - span
                startY: cy
                PathQuad { x: rightArch.cx + rightArch.span; y: rightArch.cy; controlX: rightArch.cx; controlY: rightArch.cy - art.size * .2 }
            }
        }
    }
}
