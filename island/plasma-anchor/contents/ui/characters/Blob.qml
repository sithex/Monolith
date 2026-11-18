import QtQuick

// Monolith default companion (Blob), redrawn as crisp 26 px pixel art.
// A round ivory ball following notch text colours with two upright expressive eyes.
// Tracks pointer with whole-pixel head lean and eye gaze, blinks, dozes, smiles on hover,
// and squish-bounces in whole pixels as an idle habit.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125

    readonly property int headX: gazePx(gazeX, 0.35, -1, 1)
    readonly property int headY: gazePx(gazeY, 0.45, -1, 1)
    readonly property int pupilX: gazePx(gazeX, 1.1, -3, 3)
    readonly property int pupilY: gazePx(gazeY, 1.0, -2, 2)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: open < 0.35
    readonly property bool dozing: asleep || (face ? face.lid < 0.6 : false)

    // Directional habit: hops toward pointer side
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property int habitY: habitFrame === 3 ? -2 : (habitFrame === 2 || habitFrame === 4 ? -1 : 0)
    readonly property int habitShiftX: habitFrame > 0 && habitFrame < 6 ? habitDirection : 0

    // Notch color adaptation
    readonly property color baseColor: face ? face.bodyColor : "#f1ede4"
    readonly property color eyeBaseColor: face ? face.eyeColor : "#171717"
    readonly property bool isDark: baseColor.hslLightness < 0.5

    readonly property var colors: ({
        "K": art.isDark ? Qt.darker(art.baseColor, 2.0) : Qt.darker(art.baseColor, 4.2),
        "D": art.isDark ? Qt.darker(art.baseColor, 1.35) : Qt.darker(art.baseColor, 1.30),
        "S": art.isDark ? Qt.darker(art.baseColor, 1.18) : Qt.darker(art.baseColor, 1.14),
        "B": art.baseColor,
        "L": art.isDark ? Qt.lighter(art.baseColor, 1.8) : Qt.lighter(art.baseColor, 1.05),
        "W": art.isDark ? Qt.lighter(art.baseColor, 2.5) : "#ffffff",
        "E": art.eyeBaseColor,
        "H": "#ffffff",
        "P": art.isDark ? "#d46b80" : "#ff9aa8"
    })

    readonly property var settledRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........KKKKKKKK.........",
        ".......KKLLWWWWLLKK.......",
        ".....KKLLWWWWWWLLBBKK.....",
        "....KLLWWWWWWLLBBBBBBK....",
        "...KLLWWWWLLLLBBBBBBBBK...",
        "...KLLLWWLLLLBBBBBBBBBK...",
        "..KLLLLLLBBBBBBBBBBBBBBK..",
        "..KLLBBBBBBBBBBBBBBBBBBDK.",
        "..KLBBBBBBBBBBBBBBBBBBBDK.",
        "..KBBBBBBBBBBBBBBBBBBBBDK.",
        "..KBBBBBBBBBBBBBBBBBBBBDK.",
        "..KBBBBBBBBBBBBBBBBBSSSDK.",
        "...KBBBBBBBBBBBBBBSSSDDK..",
        "...KBBBBBBBBBBBBSSSSDDDK..",
        "....KBBBBBBBSSSSSSDDDDK...",
        ".....KKBBSSSSSSDDDDDDKK...",
        ".......KKSSDDDDDDDDKK.....",
        ".........KKKKKKKKKK.......",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var squishRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "........KKKKKKKKKK........",
        "......KKLLWWWWWWLLKK......",
        "....KKLLWWWWWWWWLLBBKK....",
        "...KLLWWWWWWWWLLBBBBBBK...",
        "..KLLWWWWLLLLBBBBBBBBBBK..",
        ".KLLLLLLBBBBBBBBBBBBBBBBK.",
        ".KLBBBBBBBBBBBBBBBBBBBBBDK",
        ".KLBBBBBBBBBBBBBBBBBBBBBDK",
        ".KBBBBBBBBBBBBBBBBBBBBBBDK",
        ".KBBBBBBBBBBBBBBBBBBBBBBDK",
        ".KBBBBBBBBBBBBBBBBBSSSSDDK",
        "..KBBBBBBBBBBBBBBSSSSDDDK.",
        "...KBBBBBBBBBBBSSSSSSDDDK.",
        "....KBBBBBBBSSSSSSSDDDDK..",
        ".....KKBBSSSSSSDDDDDDDKK..",
        ".......KKSSDDDDDDDDDKK....",
        ".........KKKKKKKKKK.......",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var stretchRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........KKKKKK..........",
        "........KKLLWWLLKK........",
        "......KKLLWWWWLLBBKK......",
        ".....KLLWWWWWWLLBBBBK.....",
        "....KLLWWWWLLLLBBBBBBK....",
        "...KLLLLLLBBBBBBBBBBBBK...",
        "...KLBBBBBBBBBBBBBBBBBK...",
        "...KLBBBBBBBBBBBBBBBBBDK..",
        "...KLBBBBBBBBBBBBBBBBBDK..",
        "...KBBBBBBBBBBBBBBBBBBDK..",
        "...KBBBBBBBBBBBBBBBBBBDK..",
        "...KBBBBBBBBBBBBBBBBBBDK..",
        "...KBBBBBBBBBBBBBBSSSDDK..",
        "....KBBBBBBBBBBSSSSDDDK...",
        ".....KBBBBBBSSSSSSDDDDK...",
        "......KKBBSSSSSDDDDDDKK...",
        "........KKSSDDDDDDDDKK....",
        "..........KKKKKKKKKK......",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    Item {
        id: body
        objectName: "companionBody"
        width: 26
        height: 26
        x: art.headX
        y: art.headY + art.habitY

        PixelSprite {
            objectName: "blobSkin"
            colors: art.colors
            rows: art.habitFrame === 1 || art.habitFrame === 5 ? art.squishRows
                : (art.habitFrame === 2 || art.habitFrame === 4 ? art.stretchRows : art.settledRows)
        }

        Repeater {
            model: [8, 16]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.happy
                x: modelData + (art.habitFrame > 0 ? art.habitShiftX : art.pupilX)
                y: 11 + (art.habitFrame > 0 ? (art.habitFrame === 1 || art.habitFrame === 5 ? 1 : -1) : art.pupilY)
                    + (art.eyesShut ? 2 : art.dozing ? 1 : 0)
                colors: art.colors
                rows: art.eyesShut ? ["EE"] : art.dozing ? ["EE", "EE"] : ["EH", "EE", "EE"]
            }
        }

        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 7 + art.pupilX
            y: 11 + art.pupilY
            colors: art.colors
            rows: [
                ".EE.....EE.",
                "E..E...E..E",
                "...........",
                ".PP.....PP."
            ]
        }
    }
}
