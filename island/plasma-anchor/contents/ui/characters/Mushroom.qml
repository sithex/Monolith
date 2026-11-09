import QtQuick

// Nick's approved 26 px rendition: low, lopsided spotted dome and a wide peach body.
// An idle blink starts eight 200 ms reference-style bob steps, never a continuous loop.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 8
    habitInterval: 200

    readonly property int pupilX: gazePx(gazeX, 0.55, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.8, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: open < 0.3 || habitFrame === 8
    readonly property bool dozing: asleep || (face ? face.lid < 0.6 : false)
    readonly property int dip: [0, 1, 1, 1, 0, 0, 1, 0, 0][habitFrame]
    readonly property bool squashed: habitFrame === 2 || habitFrame === 3 || habitFrame === 6

    readonly property var colors: ({
        "K": "#3b1410", "R": "#ef4b31", "r": "#cf3523", "d": "#a8261b",
        "O": "#ff8a5c", "o": "#ffb48c", "W": "#fffaf0", "w": "#ffd2c2",
        "P": "#ff9a86", "p": "#f27a66", "U": "#7e1d14", "B": "#ffe6c2",
        "L": "#fff5e2", "b": "#ffcc94", "n": "#f2a05a", "N": "#d97c3c",
        "E": "#1e1414", "H": "#ffffff", "M": "#e86a5c"
    })

    // The settled cap and body are pixel-for-pixel the controller's approved concept.
    PixelSprite {
        objectName: "mushroomCap"
        y: art.dip
        colors: art.colors
        rows: [
            "..........KKKKKK..........",
            "........KKooooRRKK........",
            "......KKOoooRRWWRKK.......",
            ".....KOOooOORRWWWRRK......",
            "....KOOooOOORRRWWRRRK.....",
            "...KROOOOOORRRRRRRRRRK....",
            "..KRROOOOORRRRRRRRWWRRRK..",
            "..KRRRRRRRRRRRRRRWWWRRRK..",
            ".KRRPPPRRRRRRRRRRRWWRRRRK.",
            ".KRPPPPPRRRRRRRRRRRRRRRrK.",
            "KRRPPPPPRRRRRRRRRRRRRWWrrK",
            "KRRRPPPRRRRRRRRRRPPRRWWrrK",
            "KrRRRRRRRRRRRRRRPPPPRRRrrK",
            "KrrRRRRRRRRRRRRRRPPRRRrrrK",
            ".KdrrrrrRRRRRRRRRRRRrrrdK.",
            "..KKdUUUUUUUUUUUUUUUUdKK.."
        ]
    }

    Item {
        id: body
        objectName: "mushroomBody"
        y: 16 + art.dip
        width: 26
        height: art.squashed ? 8 : 9

        PixelSprite {
            objectName: "mushroomSkin"
            colors: art.colors
            readonly property var settledRows: [
                "...KBLLBBBBBBBBBBBBBBbK...",
                "..KBLLBBBBBBBBBBBBBBBbnK..",
                "..KBLBBBBBBBBBBBBBBBbbnK..",
                "..KBBBBBBBBBBBBBBBBBbbnK..",
                "..KBBBBBBBBBBBBBBBBBbbnK..",
                "..KbBBBBBBBBBBBBBBBbbnNK..",
                "...KbBBBBBBBBBBBBbbbnNK...",
                "....KKbbbbbbbbbbbbnnKK....",
                "......KKKKKKKKKKKKKK......"
            ]
            // Remove one plain belly row and duplicate two central columns, not a Scale:
            // every outline/shading run remains an integer pixel; width grows on both sides.
            rows: art.squashed ? settledRows.filter((r, i) => i !== 3).map(
                r => r.slice(1, 13) + r.slice(12, 14) + r.slice(13, 25)) : settledRows
        }

        Repeater {
            model: [7, 17]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.happy
                x: modelData + art.pupilX
                y: 1 + art.pupilY + (art.eyesShut ? 1 : 0)
                colors: art.colors
                rows: art.eyesShut ? ["EE"] : art.dozing ? ["bb", "EE", "EE"]
                    : ["EH", "EE", "EE"]
            }
        }

        PixelSprite {
            objectName: "mushroomMouth"
            visible: !art.happy
            x: 11
            y: art.squashed ? 3 : 4
            colors: art.colors
            rows: ["M.M.M", ".M.M."]
        }

        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 5
            y: 1
            colors: art.colors
            rows: [
                "..E.........E...",
                ".E.E.......E.E..",
                "pp............pp",
                ".....M.....M....",
                "......MMMMM....."
            ]
        }
    }
}
