import QtQuick

// The Knight (Hollow Knight), as crisp 1× pixel art: a white horned mask with two big
// black eye holes over a small dark cloak. The eyes slide around inside the mask to
// follow the pointer and the head leans a pixel toward it when it's far off.
//
// Hover: the Knight draws its nail and holds it up, ready. Idle habit: a quick nail
// slash toward the pointer's side, with a white arc. Dozing: eyes half shut, head bowed.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6

    // Eyes: -1…1 across and down, in whole pixels. Head lean: one more pixel, far out.
    readonly property int eyeX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, -1, 1)
    readonly property int lean: gazePx(gazeX, 0.42, -1, 1)
    // Slash toward the side the pointer is on; picked when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property int slashFrame: habitFrame
    readonly property bool slashing: !happy && slashFrame >= 1 && slashFrame <= 4
    // Head's left edge (13 px wide). Steps back from a slash to make room for the arc.
    readonly property int headX: slashing ? (habitDirection > 0 ? 1 : 6) : 3 + lean
    readonly property int bodyX: headX + 1
    readonly property int headY: art.asleep ? 1 : 0

    readonly property var maskColors: ({ "M": "#f4f1ea", "m": "#bdbac6", "o": "#0b0b10" })
    readonly property var cloakColors: ({ "C": "#3f475e", "c": "#5c6683", "k": "#2a2f3f", "L": "#2a2f3f" })
    readonly property var eyeColors: ({ "E": "#07070b" })
    readonly property var nailColors: ({ "N": "#e9e7ee", "n": "#a9a7b4", "h": "#6a5a4a", "A": "#ffffff", "a": "#c9d3ff" })

    // Cloak and legs, under the head.
    PixelSprite {
        objectName: "knightCloak"
        x: art.bodyX
        y: 13
        colors: art.cloakColors
        rows: art.happy ? [
            ".kCCCCCCCk.",
            "kCcCCCCCcCk",
            "kCcCCCCCCCk",
            ".kCCCCCCCk.",
            "..kL...Lk..",
            "...L...L..."
        ] : [
            "..kCCCCCk..",
            ".kCcCCCCcCk",
            "kCCcCCCCCCk",
            "kCCCCCCCCCk",
            ".k.L...L.k.",
            "...L...L..."
        ]
    }

    Item {
        objectName: "knightHead"
        x: art.headX
        y: art.headY
        width: 13
        height: 13
        PixelSprite {
            colors: art.maskColors
            rows: [
                "M...........M",
                "MM.........MM",
                ".M.........M.",
                ".MM.......MM.",
                "..MMMMMMMMM..",
                ".MMMMMMMMMMM.",
                "MMMMMMMMMMMMM",
                "MMMMMMMMMMMMM",
                "MMMMMMMMMMMMM",
                "MMMMMMMMMMMMm",
                "MMMMMMMMMMMMm",
                ".mMMMMMMMMMm.",
                "..mmMMMMMmm.."
            ]
        }
        // Eye holes: open, half shut while dozing, a slit for a blink.
        Repeater {
            model: [2, 7]
            PixelSprite {
                required property int modelData
                objectName: "companionEye" + (modelData === 2 ? 0 : 1)
                readonly property real open: art.face ? art.face.blink * art.face.lid : 1
                x: modelData + art.eyeX
                y: 6 + art.eyeY + (open < 0.3 ? 2 : open < 0.7 ? 1 : 0)
                colors: art.eyeColors
                rows: open < 0.3 ? ["EEEE"] : open < 0.7 ? [".EE.", "EEEE"] : [".EE.", "EEEE", "EEEE", ".EE."]
            }
        }
    }

    // Nail: held up on hover, swung in an arc during the habit.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: art.headX + 13
        y: 5
        colors: art.nailColors
        rows: [".N", ".N", ".N", ".N", ".N", ".N", "nNn", ".h", ".h"]
    }
    PixelSprite {
        objectName: "knightSlash"
        visible: art.slashing
        // Mirrored for a slash to the left.
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        x: 0
        y: 0
        colors: art.nailColors
        rows: art.slashFrame === 1 ? ["", "", "", "", ".................N", ".................N", "................N", "................N", "...............N", "..............nNn", "..............h", ".............h"]
            : art.slashFrame === 2 ? ["", "", "", "", "", "", ".................a", "..................A", "..............n....A", "..............hNNNN", "..............n....A", "..................A", ".................a"]
            : art.slashFrame === 3 ? ["", "", "", "................a", ".................A", "..................A", "..................A", "...................A", "..............n....A", "..............hNNN.A", "..............n....A", "...................A", "..................A", "..................A", ".................A", "................a"]
            : ["", "", "", "", "", "..................a", "", "...................a", "", "...................a", "", "...................a", "", "..................a"]
    }
}
