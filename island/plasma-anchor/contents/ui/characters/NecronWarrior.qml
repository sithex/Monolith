import QtQuick

// A Necron Warrior (Warhammer 40,000), as crisp 1× pixel art: a living-metal skull with a
// domed crest, deep eye sockets lit by green glow, a ribbed jaw grille and armoured
// shoulders over a glowing core. The glowing eyes slide inside their sockets toward the
// pointer.
//
// Hover: the eyes flare and fill their sockets. Idle habit: it raises its gauss flayer
// toward the pointer's side, charges it and fires a green beam. Dozing: stasis, the glow
// dims to a low ember at the bottom of each socket.
HabitCharacter {
    id: art
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
        "O": "#121417", "S": "#c9cdd2", "s": "#8d939a", "d": "#5a6068", "k": "#2a2e33",
        "H": "#eef1f4", "x": "#0b0d0f", "R": "#3b4046",
        "G": "#5cff7a", "g": "#1fae3e", "e": "#174d26"
    })

    PixelSprite {
        objectName: "warriorBody"
        y: 12
        colors: art.colors
        rows: [
            "........OkkO........",
            "..OOOOOOkddkOOOOOO..",
            ".OHSSSsOdkkdOsSSSsO.",
            "OHSSSsdOkRRkOdsSSsdO",
            "OSsssdOdkGGkdOdsssdO",
            ".OOOOOOkRkkRkOOOOOO.",
            "......OdkRRkdO......",
            "......OkRkkRkO......"
        ]
    }
    PixelSprite {
        objectName: "warriorHead"
        x: 3
        colors: art.colors
        rows: [
            "....OOOOOO....",
            "..OOHSSSSsOO..",
            ".OHSSSSSSSSsO.",
            ".OSSSSSSSSSsO.",
            "OSSSSSSSSSSSsO",
            "OSxxxxSSxxxxsO",
            "OSxxxxSSxxxxsO",
            "OsxxxxSSxxxxdO",
            ".OsSSSkkSSSdO.",
            "..OsOSOSOSdO..",
            "..OdOSOSOSdO..",
            "...OOOOOOOO..."
        ]
    }
    // Eye glow: a bright slit sliding toward the pointer inside each 4×3 socket.
    Repeater {
        model: [5, 11]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            readonly property bool flare: art.happy || art.habitFrame === 3 || art.habitFrame === 4
            readonly property bool dim: art.asleep || art.open < 0.5
            x: modelData + (flare ? 0 : dim ? 1 : 1 + art.eyeX)
            y: flare ? 5 : dim ? 7 : 6 + art.eyeY
            colors: art.colors
            rows: flare ? ["gGGg", "GGGG", "gGGg"] : art.asleep ? ["ee"] : dim ? ["gg"] : ["GG"]
        }
    }
    // The core flares on hover.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 8
        y: 15
        colors: art.colors
        rows: ["GGGG", ".GG."]
    }

    // Gauss flayer: drawn aimed right, mirrored for a shot to the left.
    Item {
        objectName: "warriorGauss"
        visible: art.firing
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            x: 7
            y: 15
            colors: art.colors
            rows: art.habitFrame === 1 ? ["OOOOOOOOOO", "OdssssdkkO", ".OkO.OO..."]
                : art.habitFrame === 2 || art.habitFrame === 5 ? ["OOOOOOOOOO", "OdgsgsgkkO", ".OkO.OO..."]
                : ["OOOOOOOOOO", "OdGgGgGkkO", ".OkO.OO..."]
        }
        PixelSprite {
            objectName: "warriorBeam"
            visible: art.habitFrame === 3 || art.habitFrame === 4
            x: 16
            y: 15
            colors: art.colors
            rows: art.habitFrame === 3 ? ["..g.", "GGGg", "..g."] : ["....", ".gg.", "...."]
        }
    }
}
