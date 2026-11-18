import QtQuick

// A little frog, as crisp 1× pixel art (26 px): big bulging eyes on top whose pupils
// follow the pointer, wide smiling mouth, green with creamy pale belly and tiny webbed feet.
//
// Pointer following: whole-pixel pupils (gazePx) inside the eye whites with white glints.
// Hover (hoverMark): happy ^^ eyes and sweet coral blush on the chubby cheeks.
// Blink: green lids close over the bulging eye domes.
// Doze: eyelids shut and a translucent sleep bubble floats above the head.
// Idle habit: a fly buzzes in on the side the pointer is on, mouth opens, sticky tongue snaps
// out and reels it in, cheeks puff in a satisfied gulp, then a calm settle back to rest.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 9
    habitInterval: 125

    readonly property int pupilX: gazePx(gazeX, 0.8, -2, 2)
    readonly property int pupilY: gazePx(gazeY, 0.8, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.5

    // Which side the fly comes from: chosen when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool hunting: !happy && habitFrame > 0
    // 1-2 fly buzzes, 3-4 tongue out, 5-6 reel in, 7-8 gulp, 9 blink.
    readonly property int tongueLength: [0, 0, 0, 4, 6, 4, 1, 0, 0, 0][habitFrame]
    readonly property var tonguePath: [[15, 12], [16, 10], [17, 8], [18, 6], [19, 5], [20, 4]]
    readonly property bool gulping: hunting && (habitFrame === 7 || habitFrame === 8)

    readonly property var colors: ({
        "K": "#0e1c10", "D": "#1c401e", "g": "#306a2c", "G": "#4ca838",
        "L": "#80d050", "H": "#b4ee76", "b": "#94b860", "B": "#d0e88e",
        "Y": "#f2fabe", "M": "#122a14", "R": "#8c2032", "N": "#204622",
        "P": "#ec5e7e", "p": "#f89aae", "W": "#f8faf2", "w": "#cad6ba",
        "E": "#0a100c", "I": "#ffffff", "T": "#f6587a", "t": "#c02a4a",
        "F": "#202428", "f": "#d6eafa", "S": "#bae0fc", "s": "#7cbcf2"
    })

    readonly property var baseRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "...KKKKK..........KKKKK...",
        "..KLLLLLK........KLLLLLK..",
        ".KLWWWWWLK.KKKK.KLWWWWWLK.",
        ".KWWWWWWWGGLKKLGGWWWWWWWK.",
        ".KWWWWWWWGGLGGLGGWWWWWWWK.",
        ".KwwwwwGGGGGGGGGGGGwwwwwK.",
        ".KGGGGGGGGGGGGGGGGGGGGGGK.",
        "KGLGGGGGGNLLGGLLNGGGGGGLGK",
        "KGLLMGGGGGGGGGGGGGGGGMLLGK",
        "KGDMMMMMMMMMMMMMMMMMMMMDGK",
        "KGgDbbBBBBBBBBBBBBBBbbDgGK",
        "KGLGGbbBBBBBBBBBBBBbbGGLGK",
        "KGGgGbbBBYYYYYYYYBBbbGgGGK",
        "KDGGGGbBBYYYYYYYYBBbGGGGDK",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        "KKDGGbBBBBBBBBBBBBBBbGGDKK",
        "KLLKDbbKKKKKKKKKKKKbbDKLLK",
        ".KLKLK..............KLKLK.",
        "..KKK................KKK..",
        ".........................."
    ]

    readonly property var gulpRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "...KKKKK..........KKKKK...",
        "..KLLLLLK........KLLLLLK..",
        ".KLWWWWWLK.KKKK.KLWWWWWLK.",
        ".KWWWWWWWGGLKKLGGWWWWWWWK.",
        ".KWWWWWWWGGLGGLGGWWWWWWWK.",
        ".KwwwwwGGGGGGGGGGGGwwwwwK.",
        ".KGGGGGGGGGGGGGGGGGGGGGGK.",
        "KGLGGGGGGNLLGGLLNGGGGGGLGK",
        "KLLLMGGGGGGGGGGGGGGGGMLLLK",
        "KKGLMMMMMMMMMMMMMMMMMMLGKK",
        "KGgDbbBbBBBBBBBBBBbBbbDgGK",
        "KGLGGbbBBBYYYYYYBBBbbGGLGK",
        "KGGgGbbBBYYYYYYYYBBbbGgGGK",
        "KDGGGGbBBYYYYYYYYBBbGGGGDK",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        "KKDGGbBBBBBBBBBBBBBBbGGDKK",
        "KLLKDbbKKKKKKKKKKKKbbDKLLK",
        ".KLKLK..............KLKLK.",
        "..KKK................KKK..",
        ".........................."
    ]

    readonly property var openMouthRightRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "...KKKKK..........KKKKK...",
        "..KLLLLLK........KLLLLLK..",
        ".KLWWWWWLK.KKKK.KLWWWWWLK.",
        ".KWWWWWWWGGLKKLGGWWWWWWWK.",
        ".KWWWWWWWGGLGGLGGWWWWWWWK.",
        ".KwwwwwGGGGGGGGGGGGwwwwwK.",
        ".KGGGGGGGGGGGGGGGGGGGGGGK.",
        "KGLGGGGGGNLLGGLLNGGGGGGLGK",
        "KGLLMGGGGGGGGKRRKGGGGMLLGK",
        "KGDMMMMMMMMMKRRRRKMMMMMDGK",
        "KGgDbbBBBBBBBBBBBBBBbbDgGK",
        "KGLGGbbBBBBBBBBBBBBbbGGLGK",
        "KGGgGbbBBYYYYYYYYBBbbGgGGK",
        "KDGGGGbBBYYYYYYYYBBbGGGGDK",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        "KKDGGbBBBBBBBBBBBBBBbGGDKK",
        "KLLKDbbKKKKKKKKKKKKbbDKLLK",
        ".KLKLK..............KLKLK.",
        "..KKK................KKK..",
        ".........................."
    ]

    readonly property var openMouthLeftRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "...KKKKK..........KKKKK...",
        "..KLLLLLK........KLLLLLK..",
        ".KLWWWWWLK.KKKK.KLWWWWWLK.",
        ".KWWWWWWWGGLKKLGGWWWWWWWK.",
        ".KWWWWWWWGGLGGLGGWWWWWWWK.",
        ".KwwwwwGGGGGGGGGGGGwwwwwK.",
        ".KGGGGGGGGGGGGGGGGGGGGGGK.",
        "KGLGGGGGGNLLGGLLNGGGGGGLGK",
        "KGLLMGGGGKRRKGGGGGGGGMLLGK",
        "KGDMMMMMKRRRRKMMMMMMMMMDGK",
        "KGgDbbBBBBBBBBBBBBBBbbDgGK",
        "KGLGGbbBBBBBBBBBBBBbbGGLGK",
        "KGGgGbbBBYYYYYYYYBBbbGgGGK",
        "KDGGGGbBBYYYYYYYYBBbGGGGDK",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        ".KDGGbBBBBBBBBBBBBBBbGGDK.",
        "KKDGGbBBBBBBBBBBBBBBbGGDKK",
        "KLLKDbbKKKKKKKKKKKKbbDKLLK",
        ".KLKLK..............KLKLK.",
        "..KKK................KKK..",
        ".........................."
    ]

    // Frog base body, cheeks, throat and feet
    PixelSprite {
        objectName: "frogBody"
        colors: art.colors
        rows: art.gulping ? art.gulpRows
            : (art.hunting && art.habitFrame >= 3 && art.habitFrame <= 6
                ? (art.habitDirection > 0 ? art.openMouthRightRows : art.openMouthLeftRows)
                : art.baseRows)
    }

    // Pupils: 2×2 with glint, sliding inside the eye whites toward the pointer.
    Repeater {
        model: [5, 19]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            visible: !art.eyesShut && !art.happy && !art.gulping
            x: (art.hunting && art.habitFrame >= 1 && art.habitFrame <= 6
                ? (index === 0 ? 5 : 19) + (art.habitDirection > 0 ? 1 : -1)
                : modelData + art.pupilX)
            y: (art.hunting && art.habitFrame >= 1 && art.habitFrame <= 6 ? 7 : 8 + art.pupilY)
            colors: art.colors
            rows: ["EI", "EE"]
        }
    }

    // Lids: shut (blink, doze) or happy ^^ on hover and gulp.
    Repeater {
        model: [3, 17]
        PixelSprite {
            required property int modelData
            visible: art.eyesShut || art.happy || art.gulping
            x: modelData
            y: 7
            colors: art.colors
            rows: art.happy || art.gulping ? [
                "GLLLLG",
                "LGMMGL",
                "GMMMMG",
                "GGGGGG"
            ] : [
                "GLLLLG",
                "GGGGGG",
                "KMMMMK",
                "GGGGGG"
            ]
        }
    }

    // Coral blush on hover.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 1
        y: 13
        colors: art.colors
        rows: [
            "PPP..................PPP",
            "Pp....................pP"
        ]
    }

    // Tongue and fly. Drawn using pure whole-pixel coordinates (no scale transforms).
    Item {
        objectName: "frogHunt"
        visible: art.hunting
        width: 26
        height: 26

        // Tongue: from mouth corner up and out toward the fly.
        Repeater {
            model: art.tongueLength
            Rectangle {
                required property int index
                readonly property var p: art.tonguePath[index]
                x: art.habitDirection > 0 ? p[0] : (25 - p[0])
                y: p[1]
                width: 1
                height: 1
                antialiasing: false
                color: index === art.tongueLength - 1 ? "#c02a4a" : "#f6587a"
            }
        }

        PixelSprite {
            objectName: "frogFly"
            readonly property var at: [[0, 0], [21, 2], [20, 3], [19, 4], [19, 4], [17, 7], [14, 11]][Math.min(6, art.habitFrame)]
            visible: art.habitFrame >= 1 && art.habitFrame <= 6 && art.habitFrame !== 4
            x: art.habitDirection > 0 ? at[0] : (25 - at[0] - 1)
            y: at[1]
            colors: art.colors
            rows: art.habitFrame % 2 ? ["f.", ".F"] : [".f", "F."]
        }
    }

    // Sleepy bubble above the head while dozing (static: nothing animates during sleep).
    PixelSprite {
        objectName: "frogBubble"
        visible: art.asleep
        x: 19
        y: 1
        colors: art.colors
        rows: [
            ".ss.",
            "sISs",
            "sSSs",
            ".ss."
        ]
    }
}
