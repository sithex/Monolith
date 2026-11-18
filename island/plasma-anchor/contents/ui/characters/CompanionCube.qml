import QtQuick

// The Weighted Companion Cube (Portal), as crisp 1× pixel art at 26 px: light silver corner
// bumpers with precision bevels, grey recessed channels with luminous pink seams, and the
// iconic pink heart on a white circular disc. It has no eyes, so it turns to face the
// pointer: the front face slides up to two pixels toward it and the side or top/bottom
// face it turns away from shows as a beveled strip, like a heavy industrial cube rotating
// in place. Every step is a whole pixel, so a pointer move that doesn't change the facing
// draws nothing.
//
// Hover (hoverMark): the heart lights up and a little heart floats above it.
// Blink: the heart squeezes in a subtle affectionate wink.
// Idle habit: a heartbeat, thump-thump, and a tiny heart drifts up into the notch.
// Dozing dims the heart; while Hermes waits for you the heart turns amber.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 7

    // Facing in whole pixels: -2…2 across, -1…2 down (the pointer is mostly below).
    readonly property int turnX: gazePx(gazeX, 1, -2, 2)
    readonly property int turnY: gazePx(gazeY, 1.2, -1, 2)
    readonly property int frontX: 2 + turnX
    readonly property int frontY: 2 + turnY
    // Heartbeat: thump-thump pulses on frames 1 and 3.
    readonly property bool beat: habitFrame === 1 || habitFrame === 3
    readonly property bool blinking: face ? face.blink < 0.5 : false

    readonly property var faceColors: ({
        "K": "#1b1d22",
        "W": "#f7f7fa",
        "w": "#e2e4ec",
        "s": "#babdc8",
        "S": "#969aa6",
        "d": "#545864",
        "c": "#3c3f4a",
        "D": "#f6f7fb",
        "p": art.happy ? "#ff7eb8" : art.asleep ? "#a8587e" : "#f266a4",
        "P": art.happy ? "#ffb0d4" : art.asleep ? "#bc6c90" : "#ff94c8"
    })
    readonly property var sideColors: ({
        "K": "#16181e",
        "W": "#acafba",
        "w": "#90949f",
        "s": "#767984",
        "S": "#5c5f6a",
        "d": "#3c3f48",
        "p": "#a64672",
        "P": "#b85480"
    })
    readonly property var topColors: ({
        "K": "#24262e",
        "W": "#ffffff",
        "w": "#eef0f6",
        "s": "#cccfd8",
        "S": "#a8acb6",
        "d": "#686c78",
        "p": "#fc7cb6",
        "P": "#ffa6d4"
    })
    readonly property var bottomColors: ({
        "K": "#121418",
        "W": "#848690",
        "w": "#6e717b",
        "s": "#585a64",
        "S": "#444650",
        "d": "#2c2e36",
        "p": "#84365c",
        "P": "#944068"
    })
    // No eyes to widen, so waiting shows in the heart: amber until you answer.
    readonly property var heartColors: art.mood === "waiting" && !art.happy ? ({
            "H": String(art.face.alertColor),
            "L": "#ffd89a",
            "h": "#d08614",
            "X": "#a66408"
        })
        : art.happy ? ({
            "H": "#ff52a0",
            "L": "#ffd0e6",
            "h": "#d6347a",
            "X": "#b0205e"
        })
        : art.asleep ? ({
            "H": "#a24672",
            "L": "#c47096",
            "h": "#803056",
            "X": "#622040"
        })
        : ({
            "H": "#ee5092",
            "L": "#ffb8d8",
            "h": "#c63470",
            "X": "#9a2052"
        })

    // Edge profile of the cube (top/bottom bumper, recessed channels, center seam).
    readonly property var edgeColumn: ["K", "W", "W", "W", "w", "s", "d", "d", "d", "d", "p", "d", "d", "d", "d", "s", "w", "W", "W", "W", "K"]

    Item {
        id: cube
        objectName: "cubePose"
        width: 26
        height: 26
        // A hop of one pixel on the first heartbeat.
        y: art.habitFrame === 1 ? -1 : 0

        // Corner cap connecting side and top/bottom on diagonal turns.
        Item {
            visible: art.turnX !== 0 && art.turnY !== 0
            x: art.turnX > 0 ? 2 : 23 + art.turnX
            y: art.turnY > 0 ? 2 : 23 + art.turnY
            width: Math.abs(art.turnX)
            height: Math.abs(art.turnY)
            clip: true
            PixelSprite {
                rows: ["KW", "WW"]
                colors: art.turnY > 0 ? art.topColors : art.bottomColors
            }
        }

        // Side face on the side it turns away from.
        Item {
            objectName: "cubeSide"
            visible: art.turnX !== 0
            x: art.turnX > 0 ? 2 : 23 + art.turnX
            y: art.frontY
            width: Math.abs(art.turnX)
            height: 21
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
            y: art.turnY > 0 ? 2 : 23 + art.turnY
            width: 21
            height: Math.abs(art.turnY)
            clip: true
            PixelSprite {
                rows: [art.edgeColumn.join(""), art.edgeColumn.join("")]
                colors: art.turnY > 0 ? art.topColors : art.bottomColors
            }
        }

        Item {
            objectName: "cubeFront"
            x: art.frontX
            y: art.frontY
            width: 21
            height: 21
            PixelSprite {
                colors: art.faceColors
                rows: [
                    "KKKKKKKKKKKKKKKKKKKKK",
                    "KWWWWsddddpddddsWWWWK",
                    "KWWwwsddddpddddswwWWK",
                    "KWwwwSsdddPdddsSwwwWK",
                    "KwwssSddcccccddSsswwK",
                    "KssSSddcDDDDDcddSSssK",
                    "KddddcDDDDDDDDDcddddK",
                    "KdddcDDDDDDDDDDDcdddK",
                    "KddcDDDDDDDDDDDDDcddK",
                    "KddcDDDDDDDDDDDDDcddK",
                    "KppPcDDDDDDDDDDDcPppK",
                    "KddcDDDDDDDDDDDDDcddK",
                    "KddcDDDDDDDDDDDDDcddK",
                    "KdddcDDDDDDDDDDDcdddK",
                    "KddddcDDDDDDDDDcddddK",
                    "KssSSddcDDDDDcddSSssK",
                    "KwwssSddcccccddSsswwK",
                    "KWwwwSsdddPdddsSwwwWK",
                    "KWWwwsddddpddddswwWWK",
                    "KWWWWsddddpddddsWWWWK",
                    "KKKKKKKKKKKKKKKKKKKKK"
                ]
            }
            PixelSprite {
                objectName: "cubeHeart"
                x: art.beat ? 6 : 7
                y: art.beat ? 6 : art.blinking ? 8 : 7
                colors: art.heartColors
                rows: art.beat ? [
                    ".HHH.HHH.",
                    "HLLHHHHHH",
                    "HHHHHHHHH",
                    "HHHHHHHHH",
                    "HHHHHHHHH",
                    ".hHHHHHh.",
                    "..hHHHh..",
                    "....X...."
                ] : art.blinking ? [
                    ".HH.HH.",
                    "HLHHHHH",
                    "HHHHHHH",
                    ".hHHHh.",
                    "...X..."
                ] : [
                    ".HH.HH.",
                    "HLHHHHH",
                    "HHHHHHH",
                    "HHHHHHH",
                    ".hHHHh.",
                    "..hHh..",
                    "...X..."
                ]
            }
        }
    }

    // A tiny heart above the cube: floats on hover, drifts up after a heartbeat.
    PixelSprite {
        objectName: "hoverMark"
        readonly property int drift: art.habitFrame >= 3 ? art.habitFrame - 3 : -1
        visible: art.happy || (drift >= 0 && drift <= 2)
        x: 20
        y: art.happy ? 0 : 2 - drift
        colors: art.heartColors
        rows: [
            "H.H",
            ".H."
        ]
    }
}
