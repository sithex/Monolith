import QtQuick

// A Necron Overlord (Warhammer 40,000) at 26 px: an imperious living-metal bust.
// Crowned with an ornate gold and lapis nemes headdress with a glowing green crown gem,
// wearing an ornate death-mask faceplate, pierced with glowing green eyes that follow the
// pointer, an imperial wesekh broad collar, a glowing dynastic chest glyph, and high
// cloak collar tips framing the shoulders.
//
// Hover: the eyes blaze white-hot, crown gem and chest glyph flare with eldritch energy.
// Idle habit: an ancient power surge: the dynastic core and crown pulse outward with
// eldritch radiance, eyes blaze, and energy settles back smoothly to rest.
// Dozing: stasis stumber; glowing eyes dim to low embers, crown and chest core dim.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 5
    habitInterval: 125

    readonly property int eyeX: gazePx(gazeX, 0.55, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool pulsing: !happy && habitFrame > 0
    readonly property bool blaze: happy || (pulsing && (habitFrame === 3 || habitFrame === 4))
    readonly property bool dim: open < 0.5

    readonly property var colors: ({
        // Dark outline & crevices
        "O": "#0c0e14",
        "k": "#141720",
        // White specular
        "W": "#ffffff",
        // Royal Gold & Brass
        "w": "#fff6b8",
        "Y": "#ffd036",
        "y": "#d69816",
        "a": "#94620a",
        "A": "#543505",
        // Royal Lapis Blue
        "c": "#5c8ef8",
        "B": "#305acc",
        "b": "#1b347e",
        // Living Metal / Necrodermis
        "H": "#eef3f8",
        "S": "#bac2cd",
        "s": "#727c8b",
        "d": "#424a56",
        "D": "#242a34",
        // Cloak collar (high imperious mantle)
        "n": "#48658c",
        "m": "#2c3e58",
        "M": "#182232",
        // Eldritch Gauss Green
        "F": "#ccffda",
        "G": "#57ff7e",
        "g": "#1cad3b",
        "e": "#105422",
        "x": "#07180c"
    })

    // Symmetrical 26x26 Overlord Bust: royal crown nemes, ornate death-mask skull,
    // wesekh broad collar, pauldrons, and high mantle collar.
    PixelSprite {
        objectName: "overlordHead"
        colors: art.colors
        rows: [
            "...........OOOO...........",
            "..........OyWWyO..........",
            "....OO...OywkkwyO...OO....",
            "...OnO..OyYkkkkYyO..OnO...",
            "..OnmO.OyYYkkkkYYyO.OmnO..",
            ".OnMMbOYYYYYAAYYYYYObMMnO.",
            ".OnMBBOyyyayaayayyyOBBMnO.",
            "OnMBBYOyywaaaaaawyyOYBBMnO",
            "OMbBYYywwaaaaaaaawwyYYBbMO",
            "OMbBYksdDDkAAAAkDDdskYBbMO",
            "OMbBYkSxxxxDssDxxxxSkYBbMO",
            "OMbBYkSxxxxSHHSxxxxSkYBbMO",
            "OMbBYksxxxxkDDkxxxxskYBbMO",
            "OMbBYkDsxxxxkkxxxxsDkYBbMO",
            ".OMbBYODsDsDssDsDsDOYBbMO.",
            ".OnMBbOODksksskskDOObBMnO.",
            "..OnmbOOODsDDDDsDOOObmnO..",
            "..OnmMOOyyYssssYyyOOMmnO..",
            ".OnmMOOyyBBYYYYBByyOOMmnO.",
            "OnmMOyYBBsSwwwwSsBBYyOMmnO",
            "OnmOyYBsdDxxxxxxDdsBYyOmnO",
            "OnMOYYsdDDxxxxxxDDdsYYOMnO",
            "OMOYysdDDkxxxxxxkDDdsyYOMO",
            "OOYyssDDDkkeggekkDDDssyYOO",
            ".OYsdDDkkkODeeDOkkkDDdsYO.",
            "..OOODDDDkOOOOOOkDDDDOOO.."
        ]
    }

    // Crown gem at the apex of the headdress.
    PixelSprite {
        objectName: "crownGem"
        x: 11
        y: 2
        colors: art.colors
        rows: art.asleep ? [
            "....",
            ".ee.",
            "...."
        ] : art.blaze ? [
            ".FF.",
            "FFFF",
            ".FF."
        ] : [
            ".GG.",
            "GGGG",
            ".gg."
        ]
    }

    // Dynastic power core embedded in the broad collar and chest armor.
    PixelSprite {
        objectName: "dynasticCore"
        x: 11
        y: 20
        colors: art.colors
        rows: art.asleep ? [
            "eeee",
            "eeee",
            "eeee",
            "eeee"
        ] : art.blaze ? [
            "FFFF",
            "FFFF",
            "FFFF",
            "egge"
        ] : art.pulsing && art.habitFrame === 2 ? [
            "GGGG",
            "GWWG",
            "GWWG",
            "egge"
        ] : [
            "gGGg",
            "gWWg",
            "gWWg",
            "egge"
        ]
    }

    // Eye glow: bright 2x2 pupils with specular white glint moving inside deep 4x4 sockets.
    Repeater {
        model: [8, 14]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            x: modelData + (art.blaze ? 0 : art.dim ? (art.asleep ? 1 : 0) : 1 + art.eyeX)
            y: art.blaze ? 10 : art.asleep ? 13 : art.dim ? 11 : 11 + art.eyeY
            colors: art.colors
            rows: art.blaze ? [
                "GGGG",
                "GFFG",
                "GFFG",
                "GGGG"
            ] : art.asleep ? [
                "ee"
            ] : art.dim ? [
                "eeee"
            ] : [
                "WG",
                "gG"
            ]
        }
    }

    // Hover mark: crown gem flares with bright white-hot eldritch corona.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy
        x: 10
        y: 1
        colors: art.colors
        rows: [
            "..FF..",
            ".FFFF.",
            ".FFFF.",
            "..FF.."
        ]
    }
}
