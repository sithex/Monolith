import QtQuick

// A Nurgling (Warhammer), as crisp 1× pixel art from the official miniature references:
// one crooked yellowed horn, small beady eyes, a broad toothy grin, a bloated paunch with
// a ripe pustule and a navel, stubby limbs. The beady eyes follow the pointer.
//
// Hover: sticks its tongue out with a gleeful squint. Idle habit: a fit of giggles while
// one of Grandfather Nurgle's flies buzzes round its horn. Dozing: eyes shut, and the fly
// settles on the horn.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 8

    readonly property int eyeX: gazePx(gazeX, 0.5, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.6, -1, 1)
    readonly property bool giggling: habitFrame > 0
    // Shakes with laughter: a pixel side to side on alternate frames.
    readonly property int bodyX: giggling && habitFrame < 8 ? (habitFrame % 2 ? -1 : 1) : 0
    readonly property bool squint: happy || giggling

    readonly property var colors: ({
        "O": "#27301a", "G": "#8fa548", "g": "#6f8436", "B": "#b9c46c", "H": "#e6dcb0", "h": "#a8996a",
        "K": "#3a1e1a", "T": "#f0e8c8", "S": "#a8475a", "s": "#d07a7a", "F": "#5f7230",
        "E": "#1a1a10", "Y": "#e8e070", "P": "#e0708a", "U": "#e8d070", "u": "#c8a850"
    })
    readonly property var flyColors: ({ "f": "#7d8a4a", "w": "#c8d6e0" })

    Item {
        objectName: "nurglingBody"
        x: art.bodyX
        width: 20
        height: 20
        PixelSprite {
            x: 1
            colors: art.colors
            rows: [
                "..........OHO.....",
                "..........OHhO....",
                ".........OHhO.....",
                "........OHhO......",
                "....OOOOHhOOO.....",
                "...OGGGGHGGGGO....",
                "..OGGGGGGGGGGGO...",
                "..OGGGGGGGGGGGO...",
                "..OGGGGGGGGGGGO...",
                ".OGKGGGGGGGGGKGO..",
                ".OGKKTKTKTKTKKGO..",
                ".OGgKKKKKKKKKgGO..",
                "OGO.OgKTKTKgGO.OGO",
                "OgGOGGgKKKgGGGOGgO",
                ".OOGBBBBBBBBBBGOO.",
                "..OGBBBBBBBBUuBGO.",
                "..OGBBBBgBBBuBBGO.",
                "..OGgBBBBBBBBBgGO.",
                "...OOgGOOOOOGgOO..",
                "...OFFFO...OFFFO.."
            ]
        }
        // Beady eyes: two pixels, a yellow glint, sliding toward the pointer.
        Repeater {
            model: [{ x: 5, rows: ["EY", "EE"] }, { x: 11, rows: ["YE", "EE"] }]
            PixelSprite {
                required property var modelData
                required property int index
                objectName: "companionEye" + index
                readonly property real open: art.face ? art.face.blink * art.face.lid : 1
                x: modelData.x + (art.squint || open < 0.5 ? 0 : art.eyeX)
                y: 7 + (art.squint || open < 0.5 ? 1 : art.eyeY)
                colors: art.colors
                rows: art.squint ? (index === 0 ? ["E.", ".E"] : [".E", "E."])
                    : open < 0.5 ? ["EE"] : modelData.rows
            }
        }
        // Tongue, poked out on hover.
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 9
            y: 12
            colors: art.colors
            rows: ["PP", "PP", ".P"]
        }
    }

    // One of Nurgle's flies: circles the horn while giggling, sits on it while dozing.
    readonly property var flyPath: [[0, 0], [15, 0], [17, 2], [16, 5], [14, 3], [15, 0], [17, 1], [16, 3], [17, 0]]
    PixelSprite {
        objectName: "nurglingFly"
        readonly property var at: art.asleep ? [13, 0] : art.flyPath[Math.min(8, art.habitFrame)]
        visible: art.asleep || (art.giggling && !art.happy)
        x: at[0]
        y: at[1]
        colors: art.flyColors
        rows: art.asleep ? ["wf"] : (art.habitFrame % 2 ? ["w.", ".f"] : [".w", "f."])
    }
}
