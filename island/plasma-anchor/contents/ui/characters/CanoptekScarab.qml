import QtQuick

// A Canoptek Scarab (Warhammer 40,000), as crisp 1× pixel art at 26 px:
// a split silver carapace with glowing green Necron glyphs, six articulated
// jointed mechanical legs, a sensor visor with glowing emerald optic tracking the
// pointer, and razor-sharp feeder mandibles.
//
// Tracking: the glowing emerald optic and its aura slide left/right/up/down within
// the dark visor aperture, with distinct whole-pixel poses for all directions.
// Hover (hoverMark): the visor flares white-green from end to end, carapace glyphs
// blaze with white energy, and mandibles spread wide in an aggressive threat display.
// Idle habit: an authentic robotic skitter on alternating tripod legs with sharp
// chittering scissor-chomps of the mandibles, settling smoothly to rest.
// Blink: the optic narrows to a sinister slit.
// Doze / Stasis: legs tuck close under the shell, runes dim to dormant embers.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 6
    habitInterval: 125
    habitCooldown: 12000

    readonly property int eyeX: gazePx(gazeX, 1.2, -3, 3)
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
        "L": "#96a0b0", // Leg metallic highlight
        "e": "#123a1e", // Stasis dormant green ember
        "o": "#186e30", // Deep green ambient glow
        "g": "#28d052", // Vibrant Necron glyph green
        "G": "#58ff7a", // Intense green eye optic core
        "W": "#c6ffd4", // Hot white-green energy flare
        "w": "#ffffff", // Pure white glint
        "X": "#080a0e", // Dark sensor visor cavity
        "x": "#181c24"  // Internal chassis joint
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

    // 3 legs per side: connected, jointed, lighter-grey lines attached to body, angled outward like crawling beetle
    readonly property var restLegRows: [
        "..........................",
        "..H....................H..",
        "..SH..................HS..",
        ".SS....................SS.",
        ".SL....................LS.",
        "..SL..................LS..",
        "...SL................LS...",
        "....Sdd..............ddS..",
        "..........................",
        "H........................H",
        "SH......................HS",
        ".SS....................SS.",
        "..SL...Sdd........ddS.LS..",
        "..........................",
        "..........................",
        ".......Sdd........ddS.....",
        "..SL..SS............SS.LS.",
        ".SS....................SS.",
        ".SL....................LS.",
        "..SL..................LS..",
        "..SH..................HS..",
        "...H..................H...",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var skitterLegsA: [
        ".H........................",
        ".SH....................H..",
        "SS....................HS..",
        ".SL....................SS.",
        "..SL...................LS.",
        "...SL.................LS..",
        "....Sdd..............ddS..",
        "..........................",
        ".........................H",
        "........................HS",
        ".......................SS.",
        ".H......Sdd......ddS..LS..",
        "SH.....SS..........SS.....",
        ".SS....Sdd........ddS.....",
        "..SL...................LS.",
        "...S..................SS..",
        "......................LS..",
        ".SL...................LS..",
        ".SL...................HS..",
        "..SL...................H..",
        "..SH......................",
        "...H......................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    readonly property var skitterLegsB: [
        "........................H.",
        "..H....................HS.",
        ".SH....................SS.",
        ".SS...................SL..",
        ".LS..................SL...",
        "..LS................SL....",
        "..Sdd..............ddS....",
        "..........................",
        "H.........................",
        "SH........................",
        ".SS.......................",
        "..SL..Sdd......ddS......H.",
        ".....SS..........SS.....HS",
        ".....Sdd........ddS....SS.",
        ".LS...................LS..",
        "..SS..................S...",
        "..LS......................",
        "..LS...................SL.",
        "..HS...................SL.",
        "..H...................SL..",
        "......................HS..",
        "......................H...",
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
        "....H................H....",
        "...SH................HS...",
        "...SL................LS...",
        "...S..................S...",
        "..........................",
        "....H................H....",
        "...SH................HS...",
        "...SL................LS...",
        "..........................",
        "..........................",
        "....H................H....",
        "...SH................HS...",
        "...SL................LS...",
        "...S..................S...",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        "..........................",
        ".........................."
    ]

    // Clear mandibles/pincers at front with lit edge (H/S) reading clearly on near-black
    readonly property var mandiblesOpen: [
        ".......HSSd....dSSH.......",
        ".......HSSs....sSSH.......",
        "........HSSs..sSSH........",
        ".........HSS..SSH.........",
        "..........HS..SH.........."
    ]

    readonly property var mandiblesChomp: [
        "........HSSd..dSSH........",
        "........HSSs..sSSH........",
        ".........HSSssSSH.........",
        "..........HSSSS...........",
        "...........HSH............"
    ]

    readonly property var mandiblesHover: [
        "......HSSd......dSSH......",
        ".....HSSd........dSSH.....",
        "....HSSd..........dSSH....",
        "....HSS............SSH....",
        "...HS................SH..."
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

    // 1. Jointed mechanical legs under carapace
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

    // 3. Sensor optic eye inside visor aperture with glow halo and whole-pixel tracking
    PixelSprite {
        objectName: "companionEye0"
        x: art.happy ? 6 : (art.sleeping ? 12 : (art.shut ? 10 + art.eyeX : 10 + art.eyeX))
        y: art.happy ? 15 : (art.sleeping ? 16 : (art.shut ? 16 : (art.eyeY > 0 ? 17 : (art.eyeY < 0 ? 15 : 16))))
        colors: art.colors
        rows: art.happy ? [
            "gGGGGGGGGGGGGg",
            "gGWWWWWWWWWWGg",
            "gGGGGGGGGGGGGg"
        ] : art.sleeping ? [
            "ee"
        ] : art.shut ? [
            "ogGGgo"
        ] : art.eyeY > 0 ? [
            "ogGwwgo"
        ] : [
            "ogGwwgo",
            "ogGGggo"
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
