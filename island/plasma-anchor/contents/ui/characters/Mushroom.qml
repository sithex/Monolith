import QtQuick

// A cute little toadstool, kawaii style: a red-to-crimson domed cap with white spots
// over a short cream stem with a tiny face — two dark bead eyes with a sparkling highlight,
// pink blush cheeks and a small mouth.
//
// Pointer tracking: pupils follow (whole pixels); the cap leans 1 px toward the pointer.
// Hover: happy closed-arc eyes (^^), brighter blush, a little smile.
// Idle habit (one-off burst from blinked(resting)): a tiny puff of 2–3 spore pixels released
// from the cap drifting upward and vanishing, settling back exactly.
// Doze: lids half closed per the brain's mood handling (no new timers).
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6
    habitInterval: 140

    readonly property int pupilX: gazePx(gazeX, 0.55, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.8, -1, 1)
    readonly property int lean: gazePx(gazeX, 0.35, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: open < 0.3
    readonly property bool dozing: asleep || (face ? face.lid < 0.6 : false)

    // Spore direction: chosen when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1

    readonly property var colors: ({
        "C": "#e82c2c",
        "c": "#cd1e1e",
        "D": "#911212",
        "d": "#460e0e",
        "W": "#ffffff",
        "S": "#fcf5e6",
        "s": "#e1d4c0",
        "E": "#141216",
        "H": "#ffffff",
        "P": "#ff91aa",
        "p": "#ff4b7d",
        "M": "#91303c",
        "m": "#c3233c"
    })

    // Cap: red domed body with white spots, crimson rim and underside.
    // Leans 1 px toward the pointer.
    PixelSprite {
        objectName: "mushroomCap"
        x: 2 + art.lean
        y: 2
        colors: art.colors
        rows: [
            "....CCCCCCCC....",
            "..CCCCCCCCCCCC..",
            ".CCCWCCCCCCWCCC.",
            "CCCCWCCCCCCWCCCC",
            "CCCCCCCCCCCCCCCC",
            "CCCCCWWCCCCWCCCC",
            "cDDDDDDDDDDDDDDc",
            ".dddddddddddddd."
        ]
    }

    // Stem: ivory cream column.
    PixelSprite {
        objectName: "mushroomStem"
        x: 6
        y: 10
        colors: art.colors
        rows: [
            "SSSSSSSS",
            "SSSSSSSS",
            "SSSSSSSS",
            "SSSSSSSS",
            "SSSSSSSS",
            "sSSSSSSs",
            ".ssssss."
        ]
    }

    // Normal blush: pink cheeks on the stem.
    PixelSprite {
        objectName: "mushroomBlush"
        visible: !art.happy
        x: 6
        y: 13
        colors: art.colors
        rows: ["P......P"]
    }

    // Normal mouth: tiny line, follows down tilt.
    PixelSprite {
        objectName: "mushroomMouth"
        visible: !art.happy
        x: 9
        y: 13 + (art.pupilY > 0 ? 1 : 0)
        colors: art.colors
        rows: ["MM"]
    }

    // Eyes: 2x2 dark beads with 1 px highlight that track the pointer.
    // When dozing, top half becomes a droopy half-lid.
    Repeater {
        model: [7, 11]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            visible: !art.eyesShut && !art.happy
            x: modelData + art.pupilX
            y: 11 + art.pupilY
            colors: art.colors
            rows: art.dozing ? ["ss", "EE"] : (art.pupilX > 0 ? ["EH", "EE"] : ["HE", "EE"])
        }
    }

    // Blink lids: closed flat line when blinking.
    Repeater {
        model: [7, 11]
        PixelSprite {
            required property int modelData
            visible: art.eyesShut && !art.happy
            x: modelData
            y: 12
            colors: art.colors
            rows: ["EE"]
        }
    }

    // Hover reaction: happy closed-arc eyes (^^), brighter blush, cute smile.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 6
        y: 11
        colors: art.colors
        rows: [
            "..E...E.",
            ".E.E.E.E",
            "p..mm..p"
        ]
    }

    // Spore puff: tiny puff of 2–3 spores released from the cap during a habit burst.
    // Drifts upward and vanishes, mirrored if looking left.
    Item {
        objectName: "mushroomSpores"
        visible: !art.happy && art.habitFrame > 0
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        Repeater {
            model: art.habitFrame === 1 ? [{"x": 14, "y": 3}, {"x": 15, "y": 2}]
                 : art.habitFrame === 2 ? [{"x": 14, "y": 1}, {"x": 15, "y": 1}, {"x": 16, "y": 2}]
                 : art.habitFrame === 3 ? [{"x": 15, "y": 0}, {"x": 16, "y": 1}, {"x": 17, "y": 1}]
                 : art.habitFrame === 4 ? [{"x": 16, "y": 0}, {"x": 17, "y": 0}]
                 : art.habitFrame === 5 ? [{"x": 17, "y": 0}]
                 : []
            Rectangle {
                required property var modelData
                x: modelData.x
                y: modelData.y
                width: 1
                height: 1
                antialiasing: false
                color: "#fff8dc"
            }
        }
    }
}
