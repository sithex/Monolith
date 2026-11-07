import QtQuick

// A cute fire salamander (Salamandra salamandra), as crisp 1× pixel art (side view):
// rounded chibi head facing the pointer, one big eye tracking with whole-pixel pupils,
// slate-charcoal body visible on dark notch backgrounds,
// vibrant fire-yellow and orange parotoid blotches and dorsal spots,
// sweet smile and rosy blush cheek, four grounded stubby legs,
// and a thick curling tail.
//
// Pointer tracking: whole sprite mirrors horizontally (whole-pixel column flip, no scale transforms)
// to face the pointer side; keeps last facing when gazeX is near centre.
// Pupil moves by whole pixels inside eye white for up/down/forward/back so all poses differ.
// Hover (hoverMark): happy ^ eye arc, brighter blush, tiny pink tongue.
// Idle habit: tongue flick burst out the front (2-3 px pink) then a tail wag (tail tip moves
// 1 px up/down over a few frames), settling back exactly to the rest frame.
// Doze: half lid (top row of eye white dark); blink: 1 px dark line.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125

    // Whole-pixel pointer tracking
    readonly property int rawGazeX: gazePx(gazeX, 0.65, -1, 1)
    readonly property int rawGazeY: gazePx(gazeY, 0.75, -1, 1)

    // Facing: 1 = right, -1 = left. When gazeX is near centre, keep the last facing.
    property int facing: 1
    onGazeXChanged: {
        if (gazeX > 0.3)
            facing = 1;
        else if (gazeX < -0.3)
            facing = -1;
    }

    // Pupil relative to facing: forward (+1), center (0), back (-1)
    readonly property int pupilDx: facing * rawGazeX
    readonly property int pupilDy: rawGazeY

    // Eye states
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.35
    readonly property bool dozing: !eyesShut && (asleep || open < 0.75)

    // Habit direction matches pointer side
    property int habitDirection: 1
    onHabitStarted: {
        habitDirection = gazeX < 0 ? -1 : 1;
        if (gazeX < -0.3)
            facing = -1;
        else if (gazeX > 0.3)
            facing = 1;
    }

    readonly property var colors: ({
        "O": "#15171c",
        "B": "#4a5263",
        "Y": "#ffd21a",
        "A": "#ff8a1a",
        "W": "#f4f6fc",
        "E": "#0b0c10",
        "P": "#f48ca0",
        "T": "#f05472",
        "M": "#1a1d24"
    })

    // Base sprite template (facing right, 20x20 slot)
    readonly property var baseRows: [
        "....................",
        "....................",
        "....................",
        "....................",
        "...........OOOOO....",
        "..........OBWWWBO...",
        ".........OBWWWWWBO..",
        "..O....OOOBWWWWWBO..",
        ".OYO.OOBYYBWWWBBBBO.",
        ".OAOOBBBBYBBBBBBBBO.",
        ".OYOBYYBBBBBYYBBPBO.",
        "..OOBBYBBBBBBYBBBMO.",
        "..OYBBBBBBBBBBBMMO..",
        "..OOOBBYYBBBBBBOO...",
        "....OOBBBBBBBBOO....",
        "....OBOOOOOOBOBO....",
        "...OBBO....OBBOBBO..",
        "...OOO.....OOO.OOO..",
        "....................",
        "...................."
    ]

    readonly property var currentBodyRows: {
        const r = baseRows.map(row => row.split(""));

        if (eyesShut) {
            // Blink: 1 px dark line on row 6
            for (let x = 12; x <= 14; x++) r[5][x] = "B";
            for (let x = 11; x <= 15; x++) r[6][x] = "O";
            for (let x = 11; x <= 15; x++) r[7][x] = "B";
            for (let x = 12; x <= 14; x++) r[8][x] = "B";
        } else if (happy) {
            // Happy ^ eye arc
            for (let x = 12; x <= 14; x++) r[5][x] = "B";
            for (let x = 11; x <= 15; x++) r[6][x] = "B";
            for (let x = 11; x <= 15; x++) r[7][x] = "B";
            for (let x = 12; x <= 14; x++) r[8][x] = "B";
            r[5][13] = "O";
            r[6][12] = "O";
            r[6][14] = "O";
        } else if (dozing) {
            // Half lid: top row of eye white dark (row 5)
            for (let x = 12; x <= 14; x++) r[5][x] = "B";
            const px = 13 + pupilDx;
            const py = 6 + pupilDy;
            for (let dy = 0; dy < 2; dy++) {
                for (let dx = 0; dx < 2; dx++) {
                    const y = py + dy, x = px + dx;
                    if (y >= 0 && y < 20 && x >= 0 && x < 20 && r[y][x] === "W")
                        r[y][x] = "E";
                }
            }
        } else {
            // Normal open eye: pupil slides by whole pixels inside eye white
            const px = 13 + pupilDx;
            const py = 6 + pupilDy;
            for (let dy = 0; dy < 2; dy++) {
                for (let dx = 0; dx < 2; dx++) {
                    const y = py + dy, x = px + dx;
                    if (y >= 0 && y < 20 && x >= 0 && x < 20 && r[y][x] === "W")
                        r[y][x] = "E";
                }
            }
        }

        // Habit frames: tongue flick out front (1-2) then tail wag (3-5), settling back (6)
        if (!happy && habitFrame > 0) {
            if (habitFrame === 1) {
                r[11][17] = "P";
                r[11][18] = "T";
            } else if (habitFrame === 2) {
                r[11][17] = "P";
                r[11][18] = "T";
                r[11][19] = "T";
            } else if (habitFrame === 3 || habitFrame === 5) {
                r[6][2] = "O";
                r[7][2] = "Y";
            } else if (habitFrame === 4) {
                r[7][2] = ".";
                r[8][1] = "O";
                r[8][2] = "O";
            }
        }

        // Horizontal flip via whole-pixel column reversal (no transforms/scaling)
        if (facing < 0) {
            return r.map(row => row.reverse().join(""));
        }
        return r.map(row => row.join(""));
    }

    Item {
        objectName: "salamanderBody"
        width: 20
        height: 20

        PixelSprite {
            colors: art.colors
            rows: art.currentBodyRows
        }

        // Bounding box target for companionEye0
        Item {
            objectName: "companionEye0"
            x: art.facing > 0 ? (13 + art.pupilDx) : (19 - (13 + art.pupilDx) - 1)
            y: 6 + art.pupilDy
            width: 2
            height: 2
            visible: !art.eyesShut && !art.happy
        }

        // Hover reaction: happy cheek blush + tiny pink tongue
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            colors: art.colors
            rows: art.facing > 0 ? [
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "................PP..",
                "..................T."
            ] : [
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "..PP................",
                ".T.................."
            ]
        }
    }
}
