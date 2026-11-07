import QtQuick

// A Canoptek Scarab (Warhammer 40,000), as crisp 1× pixel art: a split silver carapace
// with glowing green runes, six jointed legs, a sensor slit that glows green and a pair
// of mandibles. The glow in the sensor slit slides toward the pointer.
//
// Hover: the sensor slit lights from end to end and the mandibles open. Idle habit: a
// skitter, side to side on alternating legs with a chomp of the mandibles. Dozing: the
// sensor and runes dim and the legs tuck in.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6

    readonly property int eyeX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, 0, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool skittering: !happy && habitFrame > 0 && habitFrame < 6
    readonly property int bodyX: skittering ? (habitFrame % 2 ? -1 : 1) : 0
    readonly property bool chomp: happy || (skittering && habitFrame % 2 === 0)

    readonly property var colors: ({
        "O": "#121417", "S": "#c9cdd2", "s": "#8d939a", "d": "#5a6068", "H": "#eef1f4",
        "x": "#0b0d0f", "R": "#6a7078", "L": "#9aa0a7",
        "g": art.asleep ? "#174d26" : "#3fe063", "G": "#5cff7a", "e": "#174d26"
    })

    // Mirrors a left half onto the right, shading the right side darker.
    function mirror(halves) {
        const shade = { "H": "s", "S": "S", "s": "d", "R": "d" };
        return halves.map(h => h + h.split("").reverse().map(c => shade[c] || c).join(""));
    }
    readonly property var shell: mirror([
        "......OOOO",
        "....OOHSSO",
        "...OHSSgSO",
        "...OSSSSSO",
        "...OSSgSSO",
        "...OsSSSSO",
        "...OssSssO",
        "...OOOOOOO",
        "....OdRxxx",
        "....OdRxxx",
        ".....OOOOO"
    ])
    // Legs: three a side, jointed out from the carapace; two poses for the skitter, and
    // tucked in while dozing.
    readonly property var legs: mirror(asleep ? [
        "..........", "..........", "..LL......", "..........", "..LL......",
        "..........", "..........", "....L.....", "....L.....", ".........."
    ] : skittering && habitFrame % 2 ? [
        "..........", ".L........", "..LL......", "..........", ".LLL......",
        "..........", "..........", "....L.....", "....L.....", "...L......"
    ] : [
        "..........", "..........", ".LLL......", "..........", "..........",
        ".LLL......", "..........", "....L.....", "...L......", "..L......."
    ])

    Item {
        objectName: "scarabBody"
        x: art.bodyX
        y: 4
        width: 20
        height: 16
        PixelSprite { colors: art.colors; rows: art.shell }
        // Over the shell's dark outline, so each leg joins the silver.
        PixelSprite { colors: art.colors; rows: art.legs; y: 1 }
        // Mandibles.
        PixelSprite {
            y: 11
            colors: art.colors
            rows: art.mirror(art.chomp ? ["....LL....", "....L....."] : [".....LL...", "......LL.."])
        }
        // Sensor glow inside the 6×2 slit (columns 7–12).
        PixelSprite {
            objectName: "companionEye0"
            readonly property bool dim: art.asleep || art.open < 0.5
            x: art.happy ? 7 : dim ? 9 : 9 + art.eyeX
            y: art.happy ? 8 : dim ? 9 : 8 + art.eyeY
            colors: art.colors
            rows: art.happy ? ["gGGGGg", "gGGGGg"] : art.asleep ? ["ee"] : dim ? ["gg"] : ["GG"]
        }
        // Hover: the carapace runes flare.
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 6
            y: 2
            colors: art.colors
            rows: [".G....G.", "........", "G......G"]
        }
    }
}
