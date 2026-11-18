import QtQuick

// A Necron Warrior (Warhammer 40,000), as crisp 1× pixel art (26 px):
// a living-metal skull with a domed cranium, sagittal seam, deep eye sockets lit by green Gauss fire,
// a ribbed jaw grille, hunched shoulder pauldrons, and a glowing green power core in the sternum.
// The glowing eyes follow the pointer by whole pixels inside their sockets.
//
// Hover: the eyes flare and fill the sockets with blazing Gauss fire; the chest core flares white-hot.
// Idle habit: raises its Gauss flayer toward the pointer's side, charges the coils to brilliant emerald/white,
// and fires a Gauss disintegration beam. Dozing: stasis mode, the glow dims to a faint ember at the bottom of each socket.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 5

    readonly property int eyeX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    // Fire toward the side the pointer is on; picked when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool firing: !happy && habitFrame > 0

    readonly property var colors: ({
        "O": "#0e1013",
        "H": "#f0f5fa",
        "S": "#ccd4de",
        "s": "#8d97a3",
        "d": "#57616d",
        "m": "#373e47",
        "k": "#1f2329",
        "x": "#080a0d",
        "W": "#edfff2",
        "G": "#46f86e",
        "g": "#19a83d",
        "e": "#0f4720"
    })

    PixelSprite {
        objectName: "warriorBody"
        colors: art.colors
        rows: [
            "..........OOOOOO..........",
            "........OOHSSSSHOO........",
            "......OHSSSSSSSSSSsO......",
            ".....OHSSSSssSSSSSSsO.....",
            "....OOHSSSSssSSSSSSsdOO...",
            "....OSmxxxxddddxxxxmmSdO..",
            "....OSsxxxxddddxxxxssSdO..",
            "....OmsxxxxddddxxxxsmkOO..",
            "....OkdxxxxkkkkxxxxdkmdO..",
            "....OsdmdddkkkkddddmsdO...",
            ".....OsdsskskskskssdsO....",
            "......OmdkskskskskdmO.....",
            "..OOOO.OmdsksksksdmO.OOOO.",
            ".OHSSSO.OkkddddkkO.OSSSsO.",
            "OHSSSSsO.OkddddkO.OsSSSSdO",
            "OSSSSSsdOkddddddkOkdSSSSsd",
            "OSSSSSsdOkdGWWGdkOdSSSSsdO",
            ".OsdssddkOkGWWGkOkddssdsO.",
            "..OkkkddkOkkGGkkOkddkkkO..",
            "...OkdddkOkkddkkOkdddkO...",
            "....OsdssddddddddssdsO....",
            "....OkkddddddddddddkO.....",
            ".....OkkddddddddddkO......",
            "......OOddddddddOO........",
            "........OddddddO..........",
            "..........OOOOOO.........."
        ]
    }

    // Eye glow: bright 2x2 pupils with glint sliding inside 4x4 sockets.
    Repeater {
        model: [7, 15]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            readonly property bool flare: art.happy || art.habitFrame === 3 || art.habitFrame === 4
            readonly property bool dim: art.asleep || art.open < 0.5
            x: modelData + (flare ? 0 : dim ? (art.asleep ? 1 : 0) : 1 + art.eyeX)
            y: flare ? 5 : art.asleep ? 8 : dim ? 6 : 6 + art.eyeY
            colors: art.colors
            rows: flare ? [
                "gGGg",
                "GWWG",
                "GWWG",
                "gGGg"
            ] : art.asleep ? ["ee"] : dim ? ["gGGg"] : [
                "WG",
                "gG"
            ]
        }
    }

    // The core flares on hover.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 9
        y: 16
        colors: art.colors
        rows: [
            "OsWWWWsO",
            "OGWWWWGO",
            "OsGGGGsO"
        ]
    }

    // Gauss flayer: drawn aimed right, mirrored for a shot to the left.
    Item {
        objectName: "warriorGauss"
        visible: art.firing
        width: 26
        height: 26
        transform: Scale { origin.x: 13; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            x: 6
            y: 19
            colors: art.colors
            rows: art.habitFrame === 1 ? [
                ".OOOOOOOOOOOO.",
                "Osdssddssddskd",
                "Odkkdkkdkkdkmd",
                ".OOOOOOOOOOOO."
            ] : art.habitFrame === 2 || art.habitFrame === 5 ? [
                ".OOOOOOOOOOOO.",
                "Osdgssgssgsskd",
                "OdkGdkGdkGdkmd",
                ".OOOOOOOOOOOO."
            ] : [
                ".OOOOOOOOOOOO.",
                "OsdGWGWGWGWskd",
                "OdkgGkgGkgGkmd",
                ".OOOOOOOOOOOO."
            ]
        }
        PixelSprite {
            objectName: "warriorBeam"
            visible: art.habitFrame === 3 || art.habitFrame === 4
            x: 19
            y: 18
            colors: art.colors
            rows: art.habitFrame === 3 ? [
                ".......",
                "..gG...",
                ".......",
                ".......",
                "......."
            ] : [
                "..g.g..",
                ".gWGGg.",
                "gWGGGWG",
                ".gWGGg.",
                "..g.g.."
            ]
        }
    }
}
