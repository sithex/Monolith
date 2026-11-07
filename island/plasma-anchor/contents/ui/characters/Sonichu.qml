import QtQuick

// Sonichu, the fictional Sonic/Pikachu hybrid: a bright blue Sonic-style head
// with backward quills, Pikachu pointed ears (yellow with black tips), rosy red cheeks,
// a peach muzzle with a cute cheerful smile, and a lightning-bolt tail visible at the side.
//
// Pointer tracking: pupils follow the pointer inside the eye whites and the head
// shifts 1 px toward the pointer.
// Hover: closed happy ^^ eyes and a big joyful grin.
// Idle habit: a ZAP burst of electric spark pixels crackling from the cheek toward
// the pointer side. Dozing: half-lids over drowsy eyes.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 3
    habitInterval: 125

    // Pointer tracking: whole-pixel pupils and head shift.
    readonly property int pupilX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.7, -1, 1)
    readonly property int headShiftX: gazePx(gazeX, 0.45, -1, 1)
    readonly property int headShiftY: gazePx(gazeY, 0.45, -1, 1)

    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.3
    readonly property bool dozing: asleep || (open >= 0.3 && open < 0.7)

    // Habit: crackle toward the pointer side.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool zapping: !happy && habitFrame > 0

    // Palette:
    readonly property var colors: ({
        "K": "#111218",
        "E": "#111218",
        "Y": "#ffd800",
        "y": "#cca000",
        "B": "#245cd8",
        "b": "#12308c",
        "H": "#5494ff",
        "W": "#f8f8fa",
        "P": "#fcd0a0",
        "p": "#dca070",
        "R": "#ea2828",
        "M": "#e84060",
        "T": "#ffffff",
        "S": "#fff444",
        "s": "#ffffff"
    })

    // Lightning-bolt tail: stationary in background at the right.
    PixelSprite {
        objectName: "sonichuTail"
        x: 16
        y: 10
        colors: art.colors
        rows: [
            "...Y",
            "..YY",
            "..YY",
            ".YY.",
            ".YY.",
            "YYY.",
            "yYY.",
            "yY.."
        ]
    }

    // Head: shifts 1 px toward pointer.
    Item {
        objectName: "sonichuHead"
        x: 1 + art.headShiftX
        y: 1 + (art.asleep && art.headShiftY === 0 ? 1 : art.headShiftY)
        width: 18
        height: 17

        PixelSprite {
            objectName: "sonichuHeadBase"
            colors: art.colors
            rows: [
                "..KK..........KK..",
                "..KK..........KK..",
                "..YY..........YY..",
                "..YY..bHHHHb..YY..",
                "..bY.bBBBBBBb.Yb..",
                ".bBBBBBBBBBBBBBBb.",
                "bB.WWWWbBBbWWWW.Bb",
                "bB.WWWWbBBbWWWW.Bb",
                "bB.WWWWbBBbWWWW.Bb",
                "bB.WWWWbBBbWWWW.Bb",
                "bRR.PPPPPPPPPP.RRb",
                ".RR.PPPKKKKPPP.RR.",
                "..b.PPPPPPPPPP.b..",
                ".....PPKKKKPP.....",
                ".....P.PKKP.P.....",
                "......PppppP......",
                ".......bBBb......."
            ]
        }

        // Eye pupils: 2x2 sliding inside eye whites toward pointer.
        Repeater {
            model: [3, 11]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy && !art.dozing
                x: modelData + 1 + art.pupilX
                y: 7 + art.pupilY
                colors: art.colors
                rows: ["EE", "EE"]
            }
        }

        // Doze look: half-lids covering top rows with pupils showing below.
        Repeater {
            model: [3, 11]
            PixelSprite {
                required property int modelData
                visible: art.dozing && !art.eyesShut && !art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "BBBB",
                    "bbbb",
                    art.pupilX < 0 ? "EEWW" : art.pupilX > 0 ? "WWEE" : "WEEW",
                    "WWWW"
                ]
            }
        }

        // Shut lids: blink.
        Repeater {
            model: [3, 11]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut && !art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "BBBB",
                    "bbbb",
                    "BBBB",
                    "BBBB"
                ]
            }
        }

        // Happy closed eyes on hover.
        Repeater {
            model: [3, 11]
            PixelSprite {
                required property int modelData
                visible: art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "....",
                    ".KK.",
                    "K..K",
                    "...."
                ]
            }
        }

        // Big joyful grin on hover (hoverMark).
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 4
            y: 12
            colors: art.colors
            rows: [
                ".bKKKKKKb.",
                "bKTTTTTTKb",
                "bKMMMMMMKb",
                ".bKKKKKKb."
            ]
        }
    }

    // Electric ZAP spark burst, crackling from cheek toward pointer.
    Item {
        objectName: "sonichuZap"
        visible: art.zapping
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            objectName: "sonichuSpark"
            x: 16
            y: 9
            colors: art.colors
            rows: art.habitFrame === 1 ? [
                ".",
                "S",
                ".s",
                "S"
            ] : art.habitFrame === 2 ? [
                "S...",
                ".sS.",
                "..sS",
                ".S..",
                "..S."
            ] : [
                "..S",
                "...",
                "...s",
                "....",
                ".S.."
            ]
        }
    }
}
