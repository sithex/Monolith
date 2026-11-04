import QtQuick
import QtQuick.Shapes

// Original miniature rendition; see docs/companion-pack-spec.md.
HabitCharacter {
    id: art

    // Hold the last pose while inactive. Only the existing brain glides; these
    // whole-device-pixel translations and five rotation steps have no animator.
    readonly property bool tracking: !!face && visible && face.visible && face.attended && face.motion && !face.sleepy
    property real shiftX: 0
    property real shiftY: 0
    property int tilt: 0
    function updatePose() {
        if (!tracking) return;
        const x = snap(Math.max(-1, Math.min(1, face.shownX * 1.4)));
        const y = snap(Math.max(-1, Math.min(1, face.shownY * 1.8)));
        const angle = happy ? 0 : Math.max(-8, Math.min(8, 4 * Math.round(face.shownX * 1.5 + face.shownY * 0.75) + step * 4));
        if (x !== shiftX) shiftX = x;
        if (y !== shiftY) shiftY = y;
        if (angle !== tilt) tilt = angle;
    }
    onTrackingChanged: updatePose()
    onPixelChanged: updatePose()
    onStepChanged: updatePose()
    onHappyChanged: updatePose()
    Component.onCompleted: updatePose()
    Connections {
        target: art.face
        function onShownXChanged() { art.updatePose(); }
        function onShownYChanged() { art.updatePose(); }
    }

    Item {
        objectName: "cubePose"
        x: art.shiftX
        y: art.shiftY
        width: 20
        height: 20
        // Static 14px extent leaves clearance even at +/-8 degrees plus 1px.
        // Transform the complete drawing, never the static Shape paths.
        scale: 0.875
        rotation: art.tilt
        transformOrigin: Item.Center

        Rectangle {
            x: 2
            y: 2
            width: 16
            height: 16
            radius: 2
            color: "#75767e"
            antialiasing: true
            border.color: art.edge
            border.width: 1
        }

        Rectangle {
            x: 8
            y: 2
            width: 4
            height: 16
            radius: 0
            color: "#b9b9bf"
            antialiasing: true
        }

        Rectangle {
            x: 2
            y: 8
            width: 16
            height: 4
            radius: 0
            color: "#b9b9bf"
            antialiasing: true
        }

        Rectangle {
            x: 2
            y: 2
            width: 5
            height: 5
            radius: 1
            color: "#d6d5d8"
            antialiasing: true
            border.color: "#9b9ba3"
            border.width: 1
        }

        Rectangle {
            x: 13
            y: 2
            width: 5
            height: 5
            radius: 1
            color: "#d6d5d8"
            antialiasing: true
            border.color: "#9b9ba3"
            border.width: 1
        }

        Rectangle {
            x: 2
            y: 13
            width: 5
            height: 5
            radius: 1
            color: "#d6d5d8"
            antialiasing: true
            border.color: "#9b9ba3"
            border.width: 1
        }

        Rectangle {
            x: 13
            y: 13
            width: 5
            height: 5
            radius: 1
            color: "#d6d5d8"
            antialiasing: true
            border.color: "#9b9ba3"
            border.width: 1
        }

        Rectangle {
            x: 5
            y: 5
            width: 10
            height: 10
            radius: 5
            color: "#52535d"
            antialiasing: true
        }

        Rectangle {
            x: 6
            y: 6
            width: 8
            height: 8
            radius: 4
            color: "#c2a5b5"
            antialiasing: true
        }

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: art.happy ? "#ffe3ef" : art.asleep ? "#966e88" : "#ae668c"
                strokeColor: "transparent"
                strokeWidth: 0

                PathSvg {
                    path: "M10 13 C4 9 8 6 10 9 C12 6 16 9 10 13 Z"
                }

            }

        }

        Rectangle {
            x: 9
            y: 7
            width: 2
            height: 1
            radius: 0.5
            color: "#fff1f6"
            antialiasing: true
            visible: art.happy
            objectName: "hoverMark"
        }

    }

}