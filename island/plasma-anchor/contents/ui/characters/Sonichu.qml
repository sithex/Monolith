import QtQuick

// Sonichu, the electric-hedgehog Pokemon hybrid: a bright Pikachu-yellow head
// with Sonic-style quills sweeping back, tall pointed Pikachu ears with black tips,
// big emerald-green eyes with dark pupils and white glints, rosy red cheeks,
// a small black nose, cheerful smile, and a signature brown zigzag lightning-bolt tail.
//
// Pointer tracking: whole-pixel pupils and subtle head shift follow the pointer.
// Hover: joyful closed happy ^^ eyes and a big cheerful grin (hoverMark).
// Idle habit: short ZAP of bright pale-yellow and white spark pixels crackling from
// the cheek toward the pointer side. Dozing: half-lids over drowsy eyes.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 3
    habitInterval: 125

    // Pointer tracking: whole-pixel pupils and head shift.
    // Near-left and near-right gaze poses visibly differ from center.
    readonly property int pupilX: gazePx(gazeX, 1.2, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 1.0, -1, 1)
    readonly property int headShiftX: gazePx(gazeX, 0.45, -1, 1)
    readonly property int headShiftY: gazePx(gazeY, 0.45, -1, 1)

    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.3
    readonly property bool dozing: asleep || (open >= 0.3 && open < 0.7)

    // Habit: crackle toward the pointer side.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool zapping: !happy && habitFrame > 0

    // Palette sampled from reference art:
    readonly property var colors: ({
        "K": "#121218",
        "E": "#16181e",
        "Y": "#feda1a",
        "y": "#a6780a",
        "G": "#34a838",
        "W": "#f8f8fc",
        "T": "#ffffff",
        "R": "#e82828",
        "M": "#e64058",
        "B": "#9c5424",
        "b": "#6c3412",
        "S": "#fffca0",
        "s": "#ffffff"
    })

    // Lightning-bolt tail: stationary in background at the right.
    PixelSprite {
        objectName: "sonichuTail"
        x: 14
        y: 10
        colors: art.colors
        rows: [
            "..BBBB",
            "...BBB",
            "...BB.",
            "..BB..",
            ".BBB..",
            ".bB...",
            "bB....",
            "bB...."
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
                ".KK............KK.",
                "..KK..........KK..",
                "..YY....yy....YY..",
                ".yYY...yYYy...YYy.",
                "yYYYY.yYYYYy.YYYYy",
                ".yYYYYYYYYYYYYYYy.",
                "yYY.WWWWyyWWWW.YYy",
                "YYY.WWWWyyWWWW.YYY",
                "yYY.WWWWyyWWWW.YYy",
                ".yY.WWWWyyWWWW.Yy.",
                "yRR.YYYYYYYYYY.RRy",
                "yRR.YYYyKKyYYY.RRy",
                ".y..YYYYYYYYYY..y.",
                ".....yYKKKKYy.....",
                "......yWMMMy......",
                ".......yYYy.......",
                "........yy........"
            ]
        }

        // Eye pupils: 2x3 emerald iris with dark pupil & glint sliding inside eye whites.
        Repeater {
            model: [4, 10]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy && !art.dozing
                x: modelData + 1 + art.pupilX
                y: 6 + art.pupilY
                colors: art.colors
                rows: [
                    "TG",
                    "KE",
                    "GG"
                ]
            }
        }

        // Doze look: half-lids covering top rows with pupils showing below.
        Repeater {
            model: [4, 10]
            PixelSprite {
                required property int modelData
                visible: art.dozing && !art.eyesShut && !art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "YYYY",
                    "yyyy",
                    art.pupilX < 0 ? "EKWW" : art.pupilX > 0 ? "WWEK" : "WEKW",
                    "WWWW"
                ]
            }
        }

        // Shut lids: blink.
        Repeater {
            model: [4, 10]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut && !art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "YYYY",
                    "yyyy",
                    "yYYy",
                    "YYYY"
                ]
            }
        }

        // Happy closed eyes on hover.
        Repeater {
            model: [4, 10]
            PixelSprite {
                required property int modelData
                visible: art.happy
                x: modelData
                y: 6
                colors: art.colors
                rows: [
                    "YYYY",
                    "YKKY",
                    "KYYK",
                    "YYYY"
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
                ".yKKKKKKy.",
                "yKTTTTTTKy",
                "yKMMMMMMKy",
                ".yKKKKKKy."
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
