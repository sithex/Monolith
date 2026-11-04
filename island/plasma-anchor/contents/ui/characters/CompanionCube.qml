import QtQuick

// The Weighted Companion Cube (Portal), as crisp 1× pixel art: light corner blocks, grey
// bands with pink seams, and the pink heart on a white disk. It has no eyes, so it turns
// to face the pointer: the front face slides up to two pixels toward it and the side or
// top face it turns away from shows as a strip, like a box rotating in place. Every step
// is a whole pixel, so a pointer move that doesn't change the facing draws nothing.
//
// Hover: the heart lights up and a little heart floats above it. Idle habit: a
// heartbeat, thump-thump, and a tiny heart drifts up. Dozing dims the heart.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 7

    // Facing in whole pixels: -2…2 across, -1…2 down (the pointer is mostly below).
    readonly property int turnX: gazePx(gazeX, 1, -2, 2)
    readonly property int turnY: gazePx(gazeY, 1.2, -1, 2)
    readonly property int frontX: 3 + turnX
    readonly property int frontY: 2 + turnY
    // Heartbeat: 1 = big heart on that frame.
    readonly property bool beat: habitFrame === 1 || habitFrame === 3

    readonly property var faceColors: ({ "O": "#2c2d33", "W": "#e6e6ea", "w": "#b9bac1", "g": "#8a8c95", "d": "#6b6d76", "R": "#5a5c66", "D": "#f3f1f3", "p": art.happy ? "#ff9cc8" : art.asleep ? "#9a6f86" : "#e58cb6" })
    readonly property var sideColors: ({ "O": "#24252a", "W": "#a4a5ad", "w": "#86878f", "d": "#55565e", "p": "#a85a80" })
    readonly property var topColors: ({ "O": "#3a3b42", "W": "#f7f7fa", "w": "#d3d4d9", "d": "#8e9098", "p": "#f2a6c8" })
    readonly property var heartColors: art.happy ? ({ "H": "#ff5fa8", "L": "#ffd3e6" })
        : art.asleep ? ({ "H": "#a35f80", "L": "#c08aa4" })
        : ({ "H": "#ec6aa6", "L": "#ffb5d6" })

    // The edge column/row of the front, as seen on the side and top faces.
    readonly property var edgeColumn: ["O", "W", "W", "W", "w", "w", "d", "p", "d", "w", "w", "W", "W", "W", "O"]

    Item {
        id: cube
        objectName: "cubePose"
        width: 20
        height: 20
        // A hop of one pixel on the first beat.
        y: art.habitFrame === 1 ? -1 : 0

        // Side face on the side it turns away from.
        Item {
            objectName: "cubeSide"
            visible: art.turnX !== 0
            x: art.turnX > 0 ? 3 : 18 + art.turnX
            y: art.frontY
            width: Math.abs(art.turnX)
            height: 15
            clip: true
            PixelSprite {
                rows: art.edgeColumn.map(c => c + c)
                colors: art.sideColors
            }
        }
        // Top (looking down) or bottom (looking up) face.
        Item {
            objectName: "cubeTop"
            visible: art.turnY !== 0
            x: art.frontX
            y: art.turnY > 0 ? 2 : 17 + art.turnY
            width: 15
            height: Math.abs(art.turnY)
            clip: true
            PixelSprite {
                rows: [art.edgeColumn.join(""), art.edgeColumn.join("")]
                colors: art.turnY > 0 ? art.topColors : art.sideColors
            }
        }

        Item {
            objectName: "cubeFront"
            x: art.frontX
            y: art.frontY
            width: 15
            height: 15
            PixelSprite {
                colors: art.faceColors
                rows: [
                    "OOOOOOOOOOOOOOO",
                    "OWWWWwdpdwWWWWO",
                    "OWWWWwdpdwWWWWO",
                    "OWWWwgRRRgwWWWO",
                    "OWWwRRDDDRRwWWO",
                    "OwwRDDDDDDDRwwO",
                    "OddRDDDDDDDRddO",
                    "OppRDDDDDDDRppO",
                    "OddRDDDDDDDRddO",
                    "OwwRDDDDDDDRwwO",
                    "OWWwRRDDDRRwWWO",
                    "OWWWwgRRRgwWWWO",
                    "OWWWWwdpdwWWWWO",
                    "OWWWWwdpdwWWWWO",
                    "OOOOOOOOOOOOOOO"
                ]
            }
            PixelSprite {
                objectName: "cubeHeart"
                x: art.beat ? 4 : 5
                y: art.beat ? 4 : 5
                colors: art.heartColors
                rows: art.beat ? [
                    ".HH.HH.",
                    "HLHHHHH",
                    "HHHHHHH",
                    "HHHHHHH",
                    ".HHHHH.",
                    "..HHH..",
                    "...H..."
                ] : [
                    "HH.HH",
                    "LHHHH",
                    "HHHHH",
                    ".HHH.",
                    "..H.."
                ]
            }
        }
    }

    // A tiny heart above the cube: floats on hover, drifts up after a heartbeat.
    PixelSprite {
        objectName: "hoverMark"
        readonly property int drift: art.habitFrame >= 3 ? art.habitFrame - 3 : -1
        visible: art.happy || (drift >= 0 && drift <= 2)
        x: art.happy ? 15 : 16
        y: art.happy ? 0 : 2 - drift
        colors: art.heartColors
        rows: ["H.H", ".H."]
    }
}
