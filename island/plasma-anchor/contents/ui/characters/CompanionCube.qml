import QtQuick
import QtQuick.Shapes

// Original miniature rendition; see docs/companion-pack-spec.md.
HabitCharacter {
    id: art

    Item {
        width: 20
        height: 20
        // A small, bounded stepped tip; the 16 px cube stays inside its 20 px slot.
        // Only this Item's transform changes, never its static Shape geometry.
        rotation: art.step * 4
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
