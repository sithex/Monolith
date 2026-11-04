import QtQuick
import QtQuick.Shapes

// Original miniature rendition; see docs/companion-pack-spec.md.
HabitCharacter {
    id: art

    Shape {
        width: 20
        height: 20
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: "#d9cf9b"
            strokeColor: "#706b47"
            strokeWidth: 0.6

            PathSvg {
                path: "M5 6 Q2 5 3 1 L7 5 M13 5 L16 1 Q18 5 15 7 Z"
            }

        }

    }

    Rectangle {
        x: 3
        y: 5
        width: 14
        height: 14
        radius: 6
        color: "#76864c"
        antialiasing: true
        border.color: "#3b472c"
        border.width: 1
    }

    Rectangle {
        x: 5
        y: 11
        width: 10
        height: 7
        radius: 4
        color: "#a4ac66"
        antialiasing: true
    }

    Rectangle {
        x: 9
        y: 15
        width: 2
        height: 1
        radius: 0.5
        color: "#58603a"
        antialiasing: true
    }

    Rectangle {
        x: 1
        y: 10
        width: 4
        height: 6
        radius: 2
        color: "#899953"
        antialiasing: true
    }

    Rectangle {
        x: 15
        y: 10 - art.snap(art.step * art.pixel)
        width: 4
        height: 6
        radius: 2
        color: "#899953"
        antialiasing: true
    }

    Rectangle {
        x: 6 + art.face.shownX
        y: 7 + art.face.shownY
        width: 2
        height: Math.max(1, 2 * art.face.blink * art.face.lid)
        radius: 1
        color: "#211d18"
        antialiasing: true
        objectName: "companionEye0"
    }

    Rectangle {
        x: 12 + art.face.shownX
        y: 7 + art.face.shownY
        width: 2
        height: Math.max(1, 2 * art.face.blink * art.face.lid)
        radius: 1
        color: "#211d18"
        antialiasing: true
        objectName: "companionEye1"
    }

    Shape {
        width: 20
        height: 20
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: "#2d221c"
            strokeColor: "transparent"
            strokeWidth: 0

            PathSvg {
                path: "M5 10 Q10 13 15 10 Q14 15 10 14 Q6 14 5 10 Z"
            }

        }

    }

    Rectangle {
        x: 6
        y: 11
        width: 1.5
        height: 1.5
        radius: 0.2
        color: "#e2d4aa"
        antialiasing: true
    }

    Rectangle {
        x: 9
        y: 11
        width: 1.5
        height: 1.5
        radius: 0.2
        color: "#e2d4aa"
        antialiasing: true
    }

    Rectangle {
        x: 12
        y: 11
        width: 1.5
        height: 1.5
        radius: 0.2
        color: "#e2d4aa"
        antialiasing: true
    }

    Rectangle {
        x: 11
        y: 13
        width: 2
        height: 2
        radius: 1
        color: "#c68c86"
        antialiasing: true
        visible: art.happy
        objectName: "hoverMark"
    }

    Rectangle {
        x: 6
        y: 16
        width: 1
        height: 1
        radius: 0.5
        color: "#c0b96d"
        antialiasing: true
    }

}
