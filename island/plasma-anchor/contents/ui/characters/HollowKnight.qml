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
            fillColor: "#303644"
            strokeColor: art.edge
            strokeWidth: 0.5

            PathSvg {
                path: "M7 12 Q10 10 13 12 L16 19 L12 18 L10 20 L8 18 L5 19 Z"
            }

        }

    }

    Item {
        x: art.snap((art.step > 1 ? 1 : 0) * art.pixel)

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "#454d63"
                strokeColor: "transparent"
                strokeWidth: 0

                PathSvg {
                    path: "M7 12 L13 12 L15 18 L10 16 L6 19 Z"
                }

            }

        }

    }

    Item {
        width: 20
        height: 20
        x: art.snap(art.step * art.pixel)

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "#f1ede4"
                strokeColor: "#b4b1b9"
                strokeWidth: 0.5

                PathSvg {
                    path: "M6 6 Q3 3 6 1 Q4 5 8 5 L12 5 Q16 5 14 1 Q17 3 14 6 Q17 7 16 11 Q15 14 10 14 Q5 14 4 11 Q3 7 6 6 Z"
                }

            }

        }

        Rectangle {
            x: 6 + art.face.shownX
            y: 8 + art.face.shownY
            width: 2.7
            height: Math.max(1, 3.5 * art.face.blink * art.face.lid)
            radius: 1
            color: "#101118"
            antialiasing: true
            objectName: "companionEye0"
        }

        Rectangle {
            x: 11.5 + art.face.shownX
            y: 8 + art.face.shownY
            width: 2.7
            height: Math.max(1, 3.5 * art.face.blink * art.face.lid)
            radius: 1
            color: "#101118"
            antialiasing: true
            objectName: "companionEye1"
        }

    }

    Rectangle {
        x: 15
        y: 13
        width: 1
        height: 6
        radius: 0.5
        color: "#d1ced3"
        antialiasing: true
        visible: art.happy
        objectName: "hoverMark"
    }

}
