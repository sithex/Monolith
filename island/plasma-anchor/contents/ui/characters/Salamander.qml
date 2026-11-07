import QtQuick

// A cute fire salamander (Salamandra salamandra), as crisp 1× pixel art:
// glossy black body with vibrant fire-yellow parotoid blotches and dorsal spots,
// a wide friendly flat head, big round dark eyes with 1 px white specular highlights,
// a gentle curved smile, little stubby front toes peeking at the bottom, and a thick tail
// curling in at the side.
//
// Pointer tracking: dark pupils slide within the eyes; the head shifts 1 px toward the pointer.
// Hover reaction (hoverMark): happy squint with rosy cheek blush and a tiny pink tongue.
// Idle habit: a quick tongue flick toward the pointer side (mirrored via habitDirection),
// then settles back cleanly.
// Dozing: brain's half lids droop over the eyes; no extra timers.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125

    // Pointer tracking in whole pixels
    readonly property int pupilX: gazePx(gazeX, 0.65, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.75, -1, 1)
    readonly property int headShiftX: gazePx(gazeX, 0.45, -1, 1)
    readonly property int headShiftY: gazePx(gazeY, 0.45, -1, 1)

    // Head position (base at x: 2, y: 2)
    readonly property int headX: 2 + headShiftX
    readonly property int headY: 2 + headShiftY

    // Eye states
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.35
    readonly property bool dozing: !eyesShut && (asleep || open < 0.75)

    // Habit direction: chosen when the burst starts toward the pointer side
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1

    readonly property var colors: ({
        "K": "#0c0d10",
        "k": "#16181e",
        "S": "#2e3844",
        "s": "#445264",
        "Y": "#fccf05",
        "H": "#fee440",
        "O": "#d09c04",
        "E": "#384656",
        "D": "#020305",
        "W": "#ffffff",
        "M": "#242c36",
        "P": "#f48ca0",
        "p": "#d85472"
    })

    // Grounded body, curling tail, and front toes (20x20 slot)
    Item {
        objectName: "salamanderBody"
        width: 20
        height: 20
        PixelSprite {
            colors: art.colors
            rows: [
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "....................",
                "...............OYY..",
                "..............YHHHY.",
                "..............OHHHY.",
                ".....kKKKKKKk..kKYYk",
                "....kKKYYHHYKKk..kKK",
                "...kKKYYYYYYYYKKk.KK",
                "...kKKKYYYYYYKKKKkKK",
                "...kKK.kKKKKk.kKKKKk",
                "..kY.Y.kKKKKk.kY.Y..",
                "..kKKK..kKKk..kKKK..",
                "...................."
            ]
        }
    }

    // Friendly flat head: shifts 1 px toward pointer
    Item {
        objectName: "salamanderHead"
        x: art.headX
        y: art.headY
        width: 15
        height: 11

        // Base skull with parotoid gland blotches and smile
        PixelSprite {
            colors: art.colors
            rows: [
                ".OYYs.....sYYO.",
                "YHHHYsssssYHHHY",
                "OHHYK.....KYHHO",
                "kYYk.......kYYk",
                "kK...........Kk",
                "kK....s.s....Kk",
                "kKK.........KKk",
                ".kKK.......KKk.",
                "..kKKKM...MKKk.",
                "...kKK.MMM.KKk.",
                ".....kKKKKk...."
            ]
        }

        // Dark eye sockets
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                x: modelData
                y: 2
                colors: art.colors
                rows: [
                    ".EE.",
                    "EEEE",
                    "EEEE",
                    ".EE."
                ]
            }
        }

        // Moving dark pupils (whole pixels)
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy
                x: modelData + Math.max(0, Math.min(2, 1 + art.pupilX))
                y: 2 + Math.max(0, Math.min(2, 1 + art.pupilY))
                colors: art.colors
                rows: ["DD", "DD"]
            }
        }

        // Specular 1 px white highlights at top of eyes
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                visible: !art.eyesShut && !art.happy
                x: modelData + 1
                y: 2
                colors: art.colors
                rows: ["W"]
            }
        }

        // Half lids while dozing
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                visible: art.dozing
                x: modelData
                y: 2
                colors: art.colors
                rows: [
                    ".KK.",
                    "kKKk"
                ]
            }
        }

        // Shut lids while blinking
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut && !art.happy
                x: modelData
                y: 3
                colors: art.colors
                rows: ["kKKk"]
            }
        }

        // Happy squint eyes on hover
        Repeater {
            model: [2, 9]
            PixelSprite {
                required property int modelData
                visible: art.happy
                x: modelData
                y: 2
                colors: art.colors
                rows: [
                    ".ss.",
                    "s..s"
                ]
            }
        }

        // Hover reaction: happy cheek blush + tiny pink tongue
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 1
            y: 6
            colors: art.colors
            rows: [
                "PP..........PP",
                "..............",
                "....pPPp......",
                ".....PP......."
            ]
        }
    }

    // Tongue flick habit: quick burst toward pointer side
    Item {
        objectName: "salamanderHabit"
        visible: !art.happy && art.habitFrame >= 1 && art.habitFrame <= 5
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            colors: art.colors
            rows: art.habitFrame === 1 ? [
                "", "", "", "", "", "", "", "", "", "",
                ".........pP........."
            ] : art.habitFrame === 2 ? [
                "", "", "", "", "", "", "", "", "",
                "...........PP.......",
                ".........pPP........"
            ] : art.habitFrame === 3 ? [
                "", "", "", "", "", "", "", "",
                ".............HPP....",
                "...........PPP......",
                ".........pPP........"
            ] : art.habitFrame === 4 ? [
                "", "", "", "", "", "", "", "", "",
                "...........PP.......",
                ".........pPP........"
            ] : art.habitFrame === 5 ? [
                "", "", "", "", "", "", "", "", "", "",
                ".........P.........."
            ] : []
        }
    }
}
