import QtQuick

// A Necron Overlord (Warhammer 40,000), as crisp 1× pixel art: a metal skull under a
// gold-and-lapis striped nemes headdress with a green crown gem, a striped collar, and a
// staff of light at its side. A green point of light in each eye socket follows the
// pointer.
//
// Hover: the eyes blaze and the staff's blade lights up. Idle habit: the staff of light
// pulses, flaring outward once, while the eyes blaze with it. Dozing: the eyes dim to a
// low line and the staff goes dark.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 5

    readonly property int eyeX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, 0, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool pulsing: !happy && habitFrame > 0
    readonly property bool blaze: happy || (pulsing && (habitFrame === 3 || habitFrame === 4))

    readonly property var colors: ({
        "O": "#121417", "Y": "#e8c04a", "y": "#a8812a", "B": "#2f55b8", "b": "#1e3577",
        "S": "#c9cdd2", "s": "#8d939a", "d": "#5a6068", "k": "#2a2e33", "x": "#0b1f12",
        "G": "#7dffa0", "g": "#1fae3e", "e": "#1a5a2c", "H": "#eef1f4"
    })

    PixelSprite {
        objectName: "overlordHead"
        x: 1
        colors: art.colors
        rows: [
            "....OOOOOOOO....",
            "..OOYYYggYYYOO..",
            ".OYYBBBBBBBBYYO.",
            ".OYYYYYYYYYYYyO.",
            "OBBOOOOOOOOOOBbO",
            "OYYOSSSSSSSSOYyO",
            "OBBOxxxSSxxxOBbO",
            "OYYOxxxSSxxxOYyO",
            "OBBOsSSkkSSdOBbO",
            "OYYOdOSOSOSOOYyO",
            "OBBO.OOOOOO.OBbO",
            "OYYO........OYyO",
            "OBBOOOOkkOOOOBbO",
            "OYYOYBYBYBYBOYyO",
            "OOOOOYBYBYBOOOOO",
            ".OSSsOOOOOOsSsO.",
            "OSSsdOkGGkOdssdO",
            ".OOOOOkddkOOOOO.",
            ".....OOOOOO....."
        ]
    }
    // Eye lights: a bright point inside each 3×2 socket.
    Repeater {
        model: [5, 10]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            readonly property bool dim: art.asleep || art.open < 0.5
            x: modelData + (art.blaze || dim ? 0 : 1 + art.eyeX)
            y: art.blaze ? 6 : dim ? 7 : 6 + art.eyeY
            colors: art.colors
            rows: art.blaze ? ["GGG", "GGG"] : art.asleep ? ["eee"] : dim ? ["ggg"] : ["G"]
        }
    }
    // Staff of light: always held; the blade glows on hover and pulses in the habit.
    PixelSprite {
        objectName: "overlordStaff"
        x: 15
        y: 2
        colors: art.colors
        rows: [
            "..y..",
            "..Y..",
            "..y..",
            "..Y..",
            "..y..",
            "..Y..",
            "..y..",
            "..Y..",
            "..y..",
            "..Y..",
            "..y..",
            "..Y..",
            "..y.."
        ].map((r, i) => i < 3 ? art.staffTip[i] : r)
    }
    readonly property var staffTip: happy ? ["..G..", ".GGG.", "..G.."]
        : asleep ? ["..e..", ".eke.", "..e.."]
        : [["..g..", ".gGg.", "..g.."], ["..G..", ".GgG.", "..G.."], ["..G..", ".GGG.", "..G.."],
           ["G.G.G", ".GGG.", "GGGGG"], ["g.G.g", ".GGG.", "gGGGg"], ["..g..", ".gGg.", "..g.."]][Math.min(5, habitFrame)]
    // Hover: the crown gem lights too.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 8
        y: 1
        colors: art.colors
        rows: ["GG"]
    }
}
