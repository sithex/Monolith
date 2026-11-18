import QtQuick

// A Canoptek Scarab (Warhammer 40,000), as crisp 1× pixel art at 26 px:
// a split silver carapace with glowing green Necron glyphs, six articulated
// jointed mechanical legs, a sensor visor with glowing emerald optic tracking the
// pointer, and razor-sharp feeder mandibles.
//
// Tracking: the glowing optic slides left/right/up/down within the dark visor aperture.
// Hover (hoverMark): the visor flares white-green from end to end, carapace glyphs
// blaze with white energy, and mandibles spread wide in an aggressive threat display.
// Idle habit: an authentic robotic skitter on alternating tripod legs with sharp
// chittering chomps of the mandibles, settling smoothly to rest.
// Blink: the optic narrows to a sinister slit.
// Doze / Stasis: legs tuck close under the shell, runes dim to dormant embers.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125
    habitCooldown: 12000

    readonly property int eyeX: gazePx(gazeX, 0.7, -2, 2)
    readonly property int eyeY: gazePx(gazeY, 0.8, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool shut: open < 0.45
    readonly property bool sleeping: asleep || (face ? face.sleepy : false)
    readonly property bool skittering: !happy && habitFrame > 0 && habitFrame <= habitFrames
    readonly property bool chomp: happy || (skittering && (habitFrame === 2 || habitFrame === 4))

    readonly property var colors: ({
        "K": "#0e1117", // Crisp dark outline & chassis seam
        "D": "#242832", // Deep gunmetal metallic shadow
        "d": "#4a5260", // Dark living metal
        "s": "#7e8896", // Midtone silver plate
        "S": "#bac4d2", // Bright polished silver
        "H": "#f2f6fc", // Specular highlight rim
        "e": "#123a1e", // Stasis dormant green ember
        "g": "#28d052", // Vibrant Necron glyph green
        "G": "#58ff7a", // Intense green eye optic core
        "W": "#c6ffd4", // Hot white-green energy flare
        "w": "#ffffff", // Pure white glint
        "X": "#080a0e", // Dark sensor visor cavity
        "x": "#181c24", // Internal chassis joint
        "L": "#96a0b0"  // Leg metallic highlight
    })

    readonly property var normalBodyRows: [
        "..........................",
        "..........................",
        ".........KKKKKKKK.........",
        ".......KKHSSSSSSSKK.......",
        "......KHSSSSKKSSSSdKK.....",
        ".....KHSSgSKKSSgSSddKK....",
        "....KHSSssgKKgssdddKK.....",
        "....KHSSgssgKKgssgssddK...",
        "....KHSSssssKKsssssddK....",
        "....KDddssssKKssssdddK....",
        "....KHDsssssKKsssssddK....",
        "....KKDddsssKKssdddKKK....",
        "......KKDDDDDDDDDDKK......",
        "......KDdssdxxxdsddDK.....",
        ".....KKSSSSSSSSSSSSKK.....",
        "....KDXXXXXXXXXXXXXXDK....",
        "...KDXXXXXXXXXXXXXXXXDK...",
        "...KDXXXXXXXXXXXXXXXXDK...",
        "....KKDDDDDDDDDDDDDDKK....",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var sleepBodyRows: normalBodyRows.map(function(r) {
        return r.replace(/g/g, "e").replace(/G/g, "e");
    })

    readonly property var restLegRows: [
        "..........................",
        "..KK..................KK..",
        ".KLdK................KdSL.",
        ".KLdK................KdSL.",
        "KLdK..................KdSL",
        "KLdK..................KdSL",
        "KdK....................KdK",
        "..........................",
        "KLK....................KLK",
        "KLSK..................KSLK",
        ".KdSK................KdSK.",
        "..KdK................KdK..",
        "...K..................K...",
        "..........................",
        ".KLK..................KLK.",
        "KLSK..................KSLK",
        "KdSK..................KdSK",
        ".KdK..................KdK.",
        "..KK..................KK..",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var skitterLegsA: [
        "..........................",
        "..KK......................",
        ".KLdK.................KK..",
        ".KLdK................KdSL.",
        "KdK..................KdSL.",
        "......................KdK.",
        "......................KK..",
        ".....................KdSL.",
        "......................KK..",
        "KLK.......................",
        "KdSK......................",
        ".KdK......................",
        ".....................KLK..",
        "....................KdSK..",
        ".....................KdK..",
        ".KLK..................K...",
        "KdSK......................",
        ".KdK......................",
        "..KK......................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var skitterLegsB: [
        "..........................",
        "......................KK..",
        "..KK.................KdSL.",
        ".KLdK................KdSL.",
        ".KLdK.................KdK.",
        "..KdK.....................",
        "..KK......................",
        ".KLdK.....................",
        "..KK......................",
        "......................KLK.",
        ".....................KdSK.",
        "......................KdK.",
        "..KLK.....................",
        ".KdSK.....................",
        "..KdK.....................",
        "...K..................KLK.",
        ".....................KdSK.",
        "......................KdK.",
        "......................KK..",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var sleepLegRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "....K................K....",
        "...KLK..............KdSK..",
        "...KdK..............KdK...",
        "..........................",
        "..........................",
        "....K................K....",
        "...KdK..............KdK...",
        "....K................K....",
        "..........................",
        "..........................",
        "..........................",
        "....K................K....",
        "...KdK..............KdK...",
        "....K................K....",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var mandiblesOpen: [
        ".......KsSSK..KsSSK.......",
        ".......KdssK..KdssK.......",
        "........KdssKKssddK.......",
        ".........KdSK.KdSK........",
        "..........KK...KK........."
    ]

    readonly property var mandiblesChomp: [
        ".......KsSK....KsSK.......",
        "........KsSK..KsSK........",
        ".........KdssssdK.........",
        "..........KdSSdK..........",
        "...........KKKK..........."
    ]

    readonly property var mandiblesHover: [
        "......KsSK......KsSK......",
        ".....KsSK........KsSK.....",
        "......KdSK......KdSK......",
        ".......KK........KK.......",
        ".........................."
    ]

    readonly property var hoverMarkRows: [
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".......W......W...........",
        "........W....W............",
        "......W..W..W..W..........",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
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

    // 1. Jointed legs under carapace
    PixelSprite {
        objectName: "scarabLegs"
        colors: art.colors
        rows: art.sleeping ? art.sleepLegRows : (art.skittering && (art.habitFrame % 2 === 1) ? art.skitterLegsA : (art.skittering && (art.habitFrame % 2 === 0) ? art.skitterLegsB : art.restLegRows))
    }

    // 2. Main body: carapace, neck, and head visor housing
    PixelSprite {
        objectName: "scarabBody"
        colors: art.colors
        rows: art.sleeping ? art.sleepBodyRows : art.normalBodyRows
    }

    // 3. Sensor optic eye inside visor aperture
    PixelSprite {
        objectName: "companionEye0"
        x: art.happy ? 6 : (art.sleeping ? 11 : (art.shut ? 11 : 11 + art.eyeX))
        y: art.happy ? 15 : (art.sleeping ? 17 : (art.shut ? 16 : 16 + (art.eyeY > 0 ? 1 : art.eyeY < 0 ? -1 : 0)))
        colors: art.colors
        rows: art.happy ? [
            "gGGGGGGGGGGGGg",
            "gGWWWWWWWWWWGg",
            "gGGGGGGGGGGGGg"
        ] : art.sleeping ? [
            "ee"
        ] : art.shut ? [
            "gGGg"
        ] : art.eyeY > 0 ? [
            "gGwg"
        ] : [
            "gGwg",
            "gGGg"
        ]
    }

    // 4. Feeder shears / mandibles
    PixelSprite {
        objectName: "scarabMandibles"
        y: 19
        colors: art.colors
        rows: art.happy ? art.mandiblesHover : (art.chomp ? art.mandiblesChomp : art.mandiblesOpen)
    }

    // 5. Hover: carapace runes flare with intense white energy
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        colors: art.colors
        rows: art.hoverMarkRows
    }
}
