import QtQuick

// Sonichu, the electric-hedgehog Pokemon hybrid: a bright Pikachu-yellow head
// with Sonic-style spiky quills sweeping back, tall pointed Pikachu ears with black tips,
// big emerald-green eyes with dark pupils and white glints, rosy red cheeks,
// a small black nose, cheerful smile, and a signature brown zigzag lightning-bolt tail.
//
// Pointer tracking: whole-pixel pupils and subtle head shift follow the pointer.
// Hover: joyful closed happy ^^ eyes and a big cheerful grin with blushing cheeks (hoverMark).
// Idle habit: short ZAP of bright pale-yellow and white spark pixels crackling from
// the cheek toward the pointer side. Dozing: half-lids over drowsy eyes.
HabitCharacter {
    id: art
    slotSize: 26
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
        "D": "#241c10",
        "N": "#3c4054",
        "n": "#282c3c",
        "H": "#fff680",
        "Y": "#feda1a",
        "O": "#df9a0a",
        "o": "#966206",
        "W": "#fcfcfe",
        "w": "#d0d4e0",
        "L": "#4ee258",
        "G": "#24a834",
        "g": "#12681c",
        "T": "#ffffff",
        "E": "#14161c",
        "R": "#ee2424",
        "r": "#aa1414",
        "p": "#ff7676",
        "M": "#e63650",
        "m": "#9c182e",
        "B": "#9c5424",
        "b": "#6c3412",
        "S": "#fffca0",
        "s": "#ffffff"
    })

    // Lightning-bolt tail: stationary in background at the right.
    PixelSprite {
        objectName: "sonichuTail"
        x: 0
        y: 0
        colors: art.colors
        rows: [
            "..........................",
            "..........................",
            ".....................KKKK.",
            "....................KHYYYK",
            "...................KHYYYYK",
            "..................KHYYYYYK",
            "...................KKYYYYK",
            "....................KYYYYK",
            "...................KHYYYK.",
            "..................KHYYYK..",
            ".................KKYYYK...",
            "................KBBBBK....",
            "...............KBBBBK.....",
            "..............KbBBK.......",
            "..............KbBK........",
            ".............KbBK.........",
            ".............KK...........",
            "..........................",
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

    // Head: shifts 1 px toward pointer.
    Item {
        objectName: "sonichuHead"
        x: art.headShiftX
        y: art.asleep && art.headShiftY === 0 ? 1 : art.headShiftY
        width: 26
        height: 26

        PixelSprite {
            objectName: "sonichuHeadBase"
            colors: art.colors
            rows: [
                "..........................",
                "..NN..................NN..",
                ".NKKN................NKKN.",
                ".NKKN.......KK.......NKKN.",
                "..KYYK.....KHYK.....KYYK..",
                "..KHYYK...KHYYYK...KYYHK..",
                "...KOYYK.KHYYYYYK.KOYYK...",
                "...KOYYYYYYYYYYYYYYOYK....",
                "..KK.KOYYYYYYYYYYYYYOYK...",
                ".KYYK.KYYYYYYYYYYYYYOK.KK.",
                ".KHYYK.KYYYYYYYYYYYOK.KYY.",
                "..KOYYKK.KKKK.KKKK.KKOYYO.",
                ".KKKOYYK.WWWWYYWWWW.KYYYO.",
                ".KYYKKOYKWWWWYYWWWWKOYKK..",
                ".KHYYKKOYWWWWYYWWWWKOYK...",
                "..KOYYKOKWWWWYYWWWWKOK....",
                ".KK.KOYKKWWWWYYWWWWKK.....",
                ".KYYKK.KYYYYYYYYYYYYK.....",
                ".KHYYK.KRRRYYKKYYRRRK.....",
                "..KOYK.KRRRYYYYYYRRRK.....",
                "...KK..KrrrYYYYYYrrrK.....",
                "........KK.KTTTTK.KK......",
                "..........KMMMMMMK........",
                "...........KmmmmK.........",
                "............KKKK..........",
                ".........................."
            ]
        }

        // Eye pupils: emerald iris with dark pupil & glint sliding inside eye whites.
        Repeater {
            model: [9, 15]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy && !art.dozing
                x: modelData + art.pupilX
                y: 12 + art.pupilY
                colors: art.colors
                rows: [
                    "TL.",
                    "TEG",
                    "EEG",
                    "Ggg"
                ]
            }
        }

        // Doze look: half-lids covering top rows with pupils showing below.
        Repeater {
            model: [9, 15]
            PixelSprite {
                required property int modelData
                visible: art.dozing && !art.eyesShut && !art.happy
                x: modelData
                y: 12
                colors: art.colors
                rows: [
                    "YYYY",
                    "yyyy",
                    "EGww",
                    "Ggww"
                ]
            }
        }

        // Shut lids: blink.
        Repeater {
            model: [9, 15]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut && !art.happy
                x: modelData
                y: 12
                colors: art.colors
                rows: [
                    "YYYY",
                    "yyyy",
                    "yKKy",
                    "YYYY"
                ]
            }
        }

        // Happy closed eyes on hover.
        Repeater {
            model: [9, 15]
            PixelSprite {
                required property int modelData
                visible: art.happy
                x: modelData
                y: 12
                colors: art.colors
                rows: [
                    "YYYY",
                    "YKKY",
                    "KYYK",
                    "YYYY"
                ]
            }
        }

        // Big joyful grin on hover with rosy blushing cheeks (hoverMark).
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 4
            y: 17
            colors: art.colors
            rows: [
                ".KppK........KppK.",
                "KRRRRK.KKKK.KRRRRK",
                "KRRRRKKTTTTKKRRRRK",
                ".KrrKKMMMMMMKKrrK.",
                ".....KmmmmmmK.....",
                "......KKKKKK......"
            ]
        }
    }

    // Electric ZAP spark burst, crackling from cheek toward pointer.
    Item {
        objectName: "sonichuZap"
        visible: art.zapping
        width: 26
        height: 26
        transform: Scale { origin.x: 13; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            objectName: "sonichuSpark"
            x: 21
            y: 17
            colors: art.colors
            rows: art.habitFrame === 1 ? [
                "S",
                "s",
                "S"
            ] : art.habitFrame === 2 ? [
                ".S..",
                "sSS.",
                ".ssS",
                "..S."
            ] : [
                "..S.",
                ".S..",
                "...s",
                "sS.."
            ]
        }
    }
}
