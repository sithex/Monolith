import QtQuick

// A cute fire salamander (Salamandra salamandra), as crisp 1× pixel art:
// chubby charcoal-slate body visible on dark notch backgrounds,
// vibrant fire-yellow and orange parotoid blotches and dorsal spots,
// rounded wide head with big cute eyes set wide on top,
// dark pupils with 1 px white glints that follow the pointer,
// sweet smile and pink blush cheeks, four splayed legs with toes,
// and a thick tail curling around the side.
//
// Pointer tracking: whole-pixel pupils moving inside white sclera.
// Hover (hoverMark): happy squint ^^ eyes, rosy cheeks, and cute pink tongue.
// Idle habit: tongue flick burst toward the pointer side (mirrored via habitDirection).
// Doze: half lids droop over the eyes; no extra timers.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125

    // Pointer tracking in whole pixels
    readonly property int pupilX: gazePx(gazeX, 0.65, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.75, -1, 1)

    // Eye states
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.35
    readonly property bool dozing: !eyesShut && (asleep || open < 0.75)

    // Habit direction: chosen when the burst starts toward the pointer side
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1

    readonly property var colors: ({
        "O": "#1e222a",
        "B": "#444b5a",
        "H": "#5e687c",
        "Y": "#ffd21a",
        "A": "#ff8a1a",
        "W": "#f4f6fc",
        "E": "#111318",
        "P": "#f48ca0",
        "T": "#f05472",
        "M": "#242a36"
    })

    // Grounded body, splayed legs with toes, and curling tail (20x20 slot)
    Item {
        objectName: "salamanderBody"
        width: 20
        height: 20
        PixelSprite {
            colors: art.colors
            rows: [
                "...OOO........OOO...",
                "..OWWWO.OOOO.OWWWO..",
                ".OWWWWOOHHHHOOWWWWO.",
                ".OWWWWOBBHHBBOOWWWWO",
                "..OWWOBBBBBBBBOWWO..",
                ".OAAOBPOBBBBOPBOAAO.",
                "OAYYAOBBM..MBBOAYYAO",
                ".OAYAOOBBMMBBOOAYAO.",
                "..OO.OBBY..YBBO.OO..",
                "OO.O.OBYY..YYBO.O.OO",
                ".OO.OBBYA..AYBBO.OO.",
                "...OBBBBBBBBBO......",
                "...OBBYY..YYBBO..OO.",
                "..OBBBYA..AYBBO.OYYO",
                ".OO.OBBYYYYYBBOOAYAO",
                "O.O.OBBBBBBBBOAYAO..",
                "....OBBBBBBBBOYYAO..",
                ".....OBBBBBBAYYAO...",
                "......OOOOOOAYAO....",
                "...........OOOO....."
            ]
        }

        // Moving dark pupils (2x2 with 1 px white glint)
        Repeater {
            model: [3, 15]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy
                x: modelData + art.pupilX
                y: 2 + art.pupilY
                colors: art.colors
                rows: art.pupilX > 0 ? ["EW", "EE"] : ["WE", "EE"]
            }
        }

        // Shut lids while blinking
        Repeater {
            model: [2, 13]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut && !art.happy
                x: modelData
                y: 1
                colors: art.colors
                rows: [
                    ".OOO.",
                    "OBBBO",
                    "OOOOO",
                    ".BBB."
                ]
            }
        }

        // Half lids while dozing
        Repeater {
            model: [2, 13]
            PixelSprite {
                required property int modelData
                visible: art.dozing
                x: modelData
                y: 1
                colors: art.colors
                rows: [
                    ".OOO.",
                    "OBBBO"
                ]
            }
        }

        // Happy squint eyes on hover
        Repeater {
            model: [2, 13]
            PixelSprite {
                required property int modelData
                visible: art.happy
                x: modelData
                y: 1
                colors: art.colors
                rows: [
                    ".OOO.",
                    "O.O.O",
                    "OBBBO",
                    ".BBB."
                ]
            }
        }

        // Hover reaction: happy cheek blush + cute pink tongue
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 5
            y: 5
            colors: art.colors
            rows: [
                "PP......PP",
                "....TT....",
                "....PP...."
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
                "", "", "", "", "", "", "",
                "..........T........."
            ] : art.habitFrame === 2 ? [
                "", "", "", "", "", "",
                "............T.......",
                "..........TP........"
            ] : art.habitFrame === 3 ? [
                "", "", "", "", "",
                "..............W.....",
                "............PT......",
                "..........TP........"
            ] : art.habitFrame === 4 ? [
                "", "", "", "", "", "",
                "............T.......",
                "..........TP........"
            ] : art.habitFrame === 5 ? [
                "", "", "", "", "", "", "",
                "..........T........."
            ] : []
        }
    }
}
