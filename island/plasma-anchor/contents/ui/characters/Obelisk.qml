import QtQuick
import QtQuick.Shapes

// Original miniature rendition; see docs/companion-pack-spec.md.
HabitCharacter {
    id: art

    Item {
        width: 20
        height: 20

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "#222329"
                strokeColor: art.edge
                strokeWidth: 0.8

                PathSvg {
                    path: "M10 3 L16 6 L14 17 L10 19 L5 16 L4 6 Z"
                }

            }

        }

        Item {
            y: -art.snap(art.step * art.pixel)

            Shape {
                width: 20
                height: 20
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: "#3b3d45"
                    strokeColor: "transparent"
                    strokeWidth: 0

                    PathSvg {
                        path: "M10 3 L16 6 L10 8 L4 6 Z"
                    }

                }

            }

            Shape {
                width: 20
                height: 20
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: "#595b65"
                    strokeColor: "transparent"
                    strokeWidth: 0

                    PathSvg {
                        path: "M10 3 L10 8 L4 6 Z"
                    }

                }

            }

        }

        Shape {
            width: 20
            height: 20
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "#111218"
                strokeColor: "transparent"
                strokeWidth: 0

                PathSvg {
                    path: "M10 8 L16 6 L14 17 L10 19 Z"
                }

            }

        }

        Rectangle {
            x: 9
            y: 9
            width: 2
            height: 6
            radius: 1
            color: art.happy ? "#fff5ac" : art.asleep ? "#897546" : "#f3cb57"
            antialiasing: true
            objectName: "hoverMark"
        }

        Rectangle {
            x: 6
            y: 7
            width: 1
            height: 8
            radius: 0
            color: "#5a5b64"
            antialiasing: true
        }

    }

}
