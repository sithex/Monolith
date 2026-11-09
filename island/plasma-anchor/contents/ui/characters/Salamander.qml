import QtQuick

// A cute fire salamander (Salamandra salamandra), as crisp 1× pixel art (side view):
// long low body with a rounded head, clear neck, four splayed little legs with toe dots,
// and a long tapering tail that curls slightly.
// Glossy black body lightened to charcoal-blue so it reads on near-black (#0b0b0c),
// with a 1 px lighter top highlight, bold yellow-orange fire blotches in two deliberate rows down the back,
// and a parotoid bulge behind the eye.
// Big round friendly eye (black with white glint) and a small smile.
//
// Pointer tracking: mirrors horizontally (whole-pixel column reversal, no scale transforms)
// to face the pointer side; keeps last facing when gazeX is near centre.
// Eye and head tilt up/down by 1 px with pointer Y.
// Pupil glint moves with pointer X for whole-pixel gaze tracking.
// Hover (hoverMark): happy ^^ eye arc, rosy cheek blush, and tiny pink tongue flick.
// Idle habit: short stepped burst — tail wag, stepping feet, and tongue flick — settling exactly.
// Blink: closed eye slit; doze: curled up resting pose with closed eye.
HabitCharacter {
    id: art
    slotSize: 26
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
        "O": "#10141c",
        "D": "#1c222e",
        "B": "#344054",
        "H": "#607494",
        "Y": "#ffd718",
        "A": "#ff8814",
        "E": "#0a0c10",
        "W": "#ffffff",
        "M": "#10141c",
        "P": "#f888a0",
        "T": "#f04c6c"
    })

    // Base sprite template (facing right, 26x26 slot)
    readonly property var baseRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..................OO......",
        ".OO..............OYYO.OO..",
        "OYYO....OOOOOOOO.OAYAOEWEO",
        "OAYBBO.OHYYHBOYABBOBOOEEO.",
        ".OBYYABOYAABBOYAABBBBOOBBO",
        "..OBBAAYBBBBBBBBBBBBBOOBMO",
        "..OOBBBBBBBBBBBBBBBBBBOOO.",
        "...OBBBBBBBBBBBBBBBBBBO...",
        "...OBBO..ODDO..OBBO..ODD..",
        "..OBBBO..ODO..OBBBO..ODO..",
        "..OO.OO..OO...OO.OO..OO...",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var currentBodyRows: {
        const r = baseRows.map(row => row.split(""));

        // Habit frames: tail wag, stepping feet, tongue flick
        if (!happy && habitFrame > 0) {
            if (habitFrame === 1) {
                r[11][1] = "O"; r[11][2] = "O";
                r[12][0] = "O"; r[12][1] = "Y"; r[12][2] = "Y"; r[12][3] = "O";
                r[13][1] = "O"; r[13][2] = "A"; r[13][3] = "B";
                r[22][18] = "."; r[22][19] = "."; r[22][20] = ".";
                r[21][18] = "O"; r[21][19] = "B"; r[21][20] = "O";
            } else if (habitFrame === 2) {
                r[11][2] = "O"; r[11][3] = "O";
                r[12][1] = "O"; r[12][2] = "Y"; r[12][3] = "Y";
                r[17][25] = "T";
            } else if (habitFrame === 3) {
                r[12][1] = "."; r[12][2] = ".";
                r[13][0] = "O"; r[13][1] = "Y"; r[13][2] = "Y";
                r[14][1] = "O"; r[14][2] = "A";
                r[22][8] = "."; r[22][9] = "."; r[22][10] = ".";
                r[21][8] = "O"; r[21][9] = "B"; r[21][10] = "O";
            } else if (habitFrame === 4) {
                r[13][1] = "O"; r[13][2] = "Y";
                r[14][0] = "O"; r[14][1] = "A"; r[14][2] = "Y";
            } else if (habitFrame === 5) {
                r[12][1] = "O"; r[12][2] = "O";
                r[13][0] = "."; r[13][1] = "Y"; r[13][2] = "Y";
            }
        } else if (dozing) {
            // Curled up sleeping pose:
            // Tail curled in slightly, eye closed, head resting
            r[14][23] = "B"; r[14][24] = "B";
            r[15][23] = "O"; r[15][24] = "O";
            r[12][1] = "."; r[12][2] = ".";
            r[13][1] = "O"; r[13][2] = "O";
            r[14][0] = "O"; r[14][1] = "Y"; r[14][2] = "Y"; r[14][3] = "O";
        } else {
            // Head tilts up/down with rawGazeY
            if (rawGazeY < 0) {
                for (let y = 11; y <= 19; y++) {
                    for (let x = 17; x <= 25; x++) {
                        r[y][x] = baseRows[y + 1][x];
                    }
                }
                for (let x = 17; x <= 25; x++) {
                    r[20][x] = (baseRows[20][x] === "O" || baseRows[20][x] === "B") ? "." : baseRows[20][x];
                }
            } else if (rawGazeY > 0) {
                for (let y = 20; y >= 13; y--) {
                    for (let x = 17; x <= 25; x++) {
                        r[y][x] = baseRows[y - 1][x];
                    }
                }
                for (let x = 17; x <= 25; x++) {
                    r[12][x] = ".";
                }
            }

            const headShift = rawGazeY < 0 ? -1 : (rawGazeY > 0 ? 1 : 0);
            const ey = 14 + headShift;

            if (eyesShut) {
                r[ey][23] = "B"; r[ey][24] = "B";
                r[ey + 1][23] = "O"; r[ey + 1][24] = "O";
            } else if (happy) {
                r[ey][23] = "O"; r[ey][24] = "B";
                r[ey + 1][23] = "B"; r[ey + 1][24] = "O";
            } else {
                if (pupilDx > 0) {
                    r[ey][23] = "E"; r[ey][24] = "W";
                    r[ey + 1][23] = "E"; r[ey + 1][24] = "E";
                } else {
                    r[ey][23] = "W"; r[ey][24] = "E";
                    r[ey + 1][23] = "E"; r[ey + 1][24] = "E";
                }
            }
        }

        if (facing < 0) {
            return r.map(row => row.reverse().join(""));
        }
        return r.map(row => row.join(""));
    }

    Item {
        objectName: "salamanderBody"
        width: 26
        height: 26

        PixelSprite {
            colors: art.colors
            rows: art.currentBodyRows
        }

        // Hover reaction: happy cheek blush + tiny pink tongue
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            colors: art.colors
            rows: art.facing > 0 ? [
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                ".....................PP...",
                ".........................T",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                ".........................."
            ] : [
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "...PP.....................",
                "T.........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                "..........................",
                ".........................."
            ]
        }
    }
}
