import QtQuick

// The controller's exact 26 px rest pose. Head tilt, glint and tail wag use whole pixels;
// the left-facing picture is a row mirror, never a transformed/scaled sprite.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125
    habitCooldown: 14000

    readonly property int rawGazeX: gazePx(gazeX, 0.55, -1, 1)
    readonly property int rawGazeY: gazePx(gazeY, 0.8, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: open < 0.35
    readonly property bool dozing: asleep || (open >= 0.35 && open < 0.75)
    property int lastFacing: 1
    readonly property int facing: !happy && habitFrame > 0 ? habitDirection : lastFacing

    onRawGazeXChanged: {
        if (rawGazeX !== 0) lastFacing = rawGazeX;
    }
    property int habitDirection: 1
    onHabitStarted: habitDirection = rawGazeX < 0 ? -1 : rawGazeX > 0 ? 1 : lastFacing

    // Exact controller concept palette; P is used only for the hover blush.
    readonly property var colors: ({
        "K": "#0f1219", "B": "#323c52", "b": "#262e40", "h": "#56637f",
        "Y": "#ffc81e", "O": "#ff8f1f", "E": "#06070a", "H": "#ffffff",
        "P": "#f888a0"
    })
    readonly property var baseRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "....................hhh...",
        "...................hEEHh..",
        "..........hhhhhh..hBEEEYh.",
        "Y........hBYYBBBhhYYBBBBBh",
        "Bh......hBYYBBBBBYYYBBBBBB",
        "bBh...hhBBBBBBYYBBBBBBBKKb",
        ".bBhhhBBYYBBBBYYBBBBBbbbb.",
        "..bbBBBBYYBBBBBBBBBbb.....",
        "....bbbbbBBb...bbBBb......",
        "........bBBb.....bBBb.....",
        "......bbb.bb.....bbb.b....",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var currentBodyRows: {
        const r = baseRows.map(row => row.split(""));
        // Move the complete raised head one pixel, not just a detached pupil.
        const headShift = dozing ? 1 : rawGazeY;
        for (let y = 8; y <= 15; y++)
            for (let x = 18; x < 26; x++) r[y][x] = ".";
        for (let y = 8; y <= 15; y++) {
            for (let x = 18; x < 26; x++) {
                if (baseRows[y][x] !== ".") r[y + headShift][x] = baseRows[y][x];
            }
        }
        const ey = 9 + headShift;
        if (eyesShut || dozing) {
            for (let x = 20; x <= 22; x++) {
                r[ey][x] = "B";
                r[ey + 1][x] = "E";
            }
        } else if (happy) {
            // Friendly ^ eye, still framed by the dark head ring.
            r[ey][20] = "B"; r[ey][21] = "E"; r[ey][22] = "B";
            r[ey + 1][20] = "E"; r[ey + 1][21] = "B"; r[ey + 1][22] = "E";
        } else if (rawGazeY > 0) {
            r[ey][22] = "E";
            r[ey + 1][22] = "H";
        } else if (rawGazeX === 0 && rawGazeY === 0) {
            // Neutral gaze lowers the glint; looking toward the facing side restores
            // the controller's exact rest pose. The black 2x2 block stays intact.
            r[ey][22] = "E";
            r[ey + 1][22] = "H";
        }

        if (dozing) {
            // Curl the tail inward and tuck the splayed toes under the body.
            for (let y = 11; y <= 16; y++)
                for (let x = 0; x < 8; x++) r[y][x] = ".";
            r[12][5] = "Y"; r[12][6] = "h";
            r[13][4] = "h"; r[13][5] = "B"; r[13][6] = "b";
            r[14][4] = "B"; r[14][5] = "b";
            r[15][5] = "B"; r[15][6] = "B"; r[15][7] = "B";
            r[16][6] = "b"; r[16][7] = "b";
            for (let x = 0; x < 26; x++) r[18][x] = ".";
        } else if (!happy && habitFrame > 0) {
            // A bounded tail wag; the thicker root remains attached to the body.
            const wag = [0, -1, -1, 0, 1, 1, 0][habitFrame];
            for (let y = 10; y <= 17; y++) {
                for (let x = 0; x < 5; x++) {
                    const sourceY = y - wag;
                    r[y][x] = sourceY >= 11 && sourceY <= 16 ? baseRows[sourceY][x] : ".";
                }
            }
        }
        return r.map(row => (facing < 0 ? row.reverse() : row).join(""));
    }

    Item {
        objectName: "salamanderBody"
        width: 26
        height: 26
        PixelSprite {
            colors: art.colors
            rows: art.currentBodyRows
        }
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: art.facing > 0 ? 23 : 1
            y: 12 + art.rawGazeY
            colors: art.colors
            rows: ["PP"]
        }
    }
}
