import QtQuick
import QtQuick.Shapes

// Original miniature rendition; see docs/companion-pack-spec.md.
HabitCharacter {
    id: art

    Item {
        width: 20
        height: 20
        y: art.snap((2 - art.step) * art.pixel)

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "#17171b"
                strokeColor: art.edge
                strokeWidth: 0.7

                PathSvg {
                    path: "M2 2 Q10 -1 18 2 L18 13 Q18 17 14 17 L6 17 Q2 17 2 13 Z"
                }

            }

        }

        Rectangle {
            x: 6 + art.face.shownX
            y: 6 + art.face.shownY
            width: 2.5
            height: Math.max(1, 3.5 * art.face.blink * art.face.lid)
            radius: 1
            color: "#f1ede4"
            antialiasing: true
            objectName: "companionEye0"
        }

        Rectangle {
            x: 12 + art.face.shownX
            y: 6 + art.face.shownY
            width: 2.5
            height: Math.max(1, 3.5 * art.face.blink * art.face.lid)
            radius: 1
            color: "#f1ede4"
            antialiasing: true
            objectName: "companionEye1"
        }

        Rectangle {
            x: 6
            y: 12
            width: 9
            height: 3
            radius: 1
            color: "#070709"
            antialiasing: true
            visible: art.happy
            objectName: "hoverMark"
        }

        Item {
            visible: art.happy

            Shape {
                width: 20
                height: 20
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: "#f1ede4"
                    strokeColor: "transparent"
                    strokeWidth: 0

                    PathSvg {
                        path: "M6.5 12 L7.5 14 L8.5 12 Z"
                    }

                }

            }

            Shape {
                width: 20
                height: 20
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: "#f1ede4"
                    strokeColor: "transparent"
                    strokeWidth: 0

                    PathSvg {
                        path: "M9 12 L10 14 L11 12 Z"
                    }

                }

            }

            Shape {
                width: 20
                height: 20
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: "#f1ede4"
                    strokeColor: "transparent"
                    strokeWidth: 0

                    PathSvg {
                        path: "M11.5 12 L12.5 14 L13.5 12 Z"
                    }

                }

            }

        }

    }

    Rectangle {
        x: 0
        y: 13
        width: 4
        height: 5
        radius: 2
        color: "#303036"
        antialiasing: true
    }

    Rectangle {
        x: 16
        y: 13
        width: 4
        height: 5
        radius: 2
        color: "#303036"
        antialiasing: true
    }

    Rectangle {
        x: 1
        y: 14
        width: 1
        height: 3
        radius: 0.5
        color: "#b7b2a7"
        antialiasing: true
    }

    Rectangle {
        x: 3
        y: 14
        width: 1
        height: 3
        radius: 0.5
        color: "#b7b2a7"
        antialiasing: true
    }

    Rectangle {
        x: 16
        y: 14
        width: 1
        height: 3
        radius: 0.5
        color: "#b7b2a7"
        antialiasing: true
    }

    Rectangle {
        x: 18
        y: 14
        width: 1
        height: 3
        radius: 0.5
        color: "#b7b2a7"
        antialiasing: true
    }

}
