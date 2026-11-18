import QtQuick

// Notch Mimic: a mimic hiding as part of the notch.
// Sleek dark notch-pill casing with top camera sensor, sharp ivory fangs, tongue,
// and glowing eyes peeking out, with little clawed hands gripping the bottom ledge.
//
// Tracking: 2x2 pupils follow the pointer in whole pixels.
// Hover (hoverMark): jaws snap wide open into a massive toothy maw with tongue and manic eyes.
// Blink: eyes close into horizontal dark creases.
// Doze: heavy eyelids droop over sleepy pupils.
// Idle habit: peeks upward (rising over the notch ledge) while claws grip the edge.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125

    readonly property int pupilX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.7, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: open < 0.35
    readonly property bool dozing: asleep || (open >= 0.35 && open < 0.65)
    readonly property int step: [0, 1, 2, 1, 2, 1, 0][Math.min(6, habitFrame)]

    readonly property var colors: ({
        // Notch casing / mimic body (dark slate / obsidian)
        "K": "#0b0c10",
        "D": "#15171f",
        "M": "#232634",
        "L": "#34394c",
        "R": "#505874",
        "H": "#848eb2",
        "B": "#afbae6",

        // Disguise camera sensor
        "S": "#101219",
        "s": "#1c202c",

        // Eyes & Teeth
        "W": "#f8f4ea",
        "w": "#c0baaa",
        "G": "#ffffff",
        "E": "#0a0b0f",

        // Oral cavity & Tongue
        "X": "#100408",
        "x": "#24090f",
        "T": "#ee3652",
        "t": "#a01c30",
        "h": "#ff88a0",

        // Claws
        "C": "#f8f4ea",
        "c": "#b4ae9e",
        "k": "#2a2e3e",
        "p": "#3c4156"
    })

    readonly property var bodyRows: [
        "..........................",
        "......KHHHHHHHHHHHHHK.....",
        "....KBLLLLLLssSSLLLLLRRK..",
        "...KRLLMMMMMssSSMMMMMMLRK.",
        "..KRLLMMMMMMMMMMMMMMMMMLRK",
        "..KLMMKKKKKKMMMMKKKKKKMMMK",
        "..KLMKKWWWWKKMMKKWWWWKKMDK",
        "..KLMKKWWWWKKMMKKWWWWKKMDK",
        "..KLMKKwwwwKKMMKKwwwwKKMDK",
        "..KLMMKKKKKKMMMMKKKKKKMMDK",
        "..KLMMMMMMMMMMMMMMMMMMMMDK",
        "..KMKWWXWWXWWXWWXWWXWWKKMK",
        "..KDKX.W..w..W..w..W.XKDMK",
        "..KDKX..W..xxTThx..W.XKDMK",
        "..KMKXWWXWWWWXWWWWXWWXKMDK",
        "..KMKKKKKKKKKKKKKKKKKKKKMK",
        "..KMDDDDDDDDDDDDDDDDDDDMDK",
        "..KKDDDDDDDDDDDDDDDDDDDDKK",
        "...KDDDDDDDDDDDDDDDDDDDK..",
        "....KKKKKKKKKKKKKKKKKKK..."
    ]

    readonly property var bodyHappyRows: [
        "..........................",
        "......KHHHHHHHHHHHHHK.....",
        "....KBLLLLLLssSSLLLLLRRK..",
        "...KRLLMMMMMssSSMMMMMMLRK.",
        "..KRLLMMMMMMMMMMMMMMMMMLRK",
        "..KLMMKKKKKKMMMMKKKKKKMMMK",
        "..KLMKKWGGEKKMMKKWGGEKKMDK",
        "..KLMKKEWWGKKMMKKEWWGKKMDK",
        "..KLMKKWGGEKKMMKKWGGEKKMDK",
        "..KLMMKKKKKKMMMMKKKKKKMMDK",
        "..KL..................DK..",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..KM..................DK..",
        "..KKDDDDDDDDDDDDDDDDDDDDKK",
        "...KDDDDDDDDDDDDDDDDDDDK..",
        "....KKKKKKKKKKKKKKKKKKK..."
    ]

    // Body rises by up to 2 px during habit, while claws grip the ledge
    Item {
        id: body
        objectName: "mimicBody"
        x: 0
        y: 2 - art.step
        width: 26
        height: 20

        PixelSprite {
            objectName: "mimicSkin"
            colors: art.colors
            rows: art.happy ? art.bodyHappyRows : art.bodyRows
        }

        Repeater {
            model: [6, 16]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.happy
                x: modelData + (art.eyesShut || art.dozing ? 0 : 1 + art.pupilX)
                y: 6 + (art.eyesShut || art.dozing ? 0 : (art.pupilY > 0 ? 1 : 0))
                colors: art.colors
                rows: art.eyesShut ? [
                    "MMMM",
                    "KKKK",
                    "DDDD"
                ] : art.dozing ? [
                    "MMMM",
                    "DDDD",
                    "EwwE"
                ] : [
                    "GE",
                    "EE"
                ]
            }
        }

        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 1
            y: 10
            colors: art.colors
            rows: [
                "KMKKKKKKKKKKKKKKKKKKKKMK",
                "KDKWWXWWXWWXWWXWWXWWXKDK",
                "KDKX.W..W..W..W..W..WXKDK",
                "KDKXxxxxTThhTThxxxxxxXKDK",
                "KDKXxxxtTTTTTTTtxxxxxXKDK",
                "KDKX..W...W...W...W..XKDK",
                "KMKXWWXWWWWXWWWWXWWXKKMK"
            ]
        }
    }

    // Hands gripping the bottom edge (anchored to bottom of slot)
    PixelSprite {
        objectName: "mimicHands"
        x: 0
        y: 20
        colors: art.colors
        rows: [
            ".p.p.p..............p.p.p.",
            ".k.k.k..............k.k.k.",
            ".C.C.C..............C.C.C.",
            "..C.C.c............c.C.C..",
            "..K.K.K............K.K.K.."
        ]
    }
}
