import QtQuick

// The Eye of Sauron (The Lord of the Rings), as crisp 1× pixel art: a lidless, flame-
// rimmed eye with a black cat's slit, held between the two curved prongs at the top of
// Barad-dûr. The slit slides across the burning iris toward the pointer.
//
// Hover: the Eye has seen you; the slit dilates and the flames leap up. Idle habit: the
// iris flares white-hot and it sweeps its searchlight toward the pointer's side. Dozing
// (it never truly sleeps): the eye narrows to a smouldering slit and the flames die down.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 5

    readonly property int pupilX: gazePx(gazeX, 1, -2, 2)
    readonly property int pupilY: gazePx(gazeY, 0.7, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool shut: asleep || open < 0.5
    // Sweep toward the side the pointer is on; picked when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool searching: !happy && habitFrame > 0
    readonly property bool charged: searching && habitFrame <= 4

    readonly property var colors: ({
        "r": "#8e1b08", "R": "#c8320c", "o": "#ff6a14", "y": "#ffb02e", "Y": "#ffe066",
        "W": "#fff6c0", "x": "#0a0403", "k": "#4a1006",
        "S": "#8a7f76", "s": "#615850", "d": "#463e38",
        "f": "#ff8a1e", "F": "#d84a12", "B": "#fff3a0", "b": "#c9a23a"
    })

    // Barad-dûr's spire top: two horns curving in around the Eye over a battlemented stem.
    PixelSprite {
        objectName: "sauronTower"
        y: 3
        colors: art.colors
        rows: [
            "..s.............s..",
            ".sS.............Ss.",
            ".S...............S.",
            ".S...............S.",
            ".Ss.............sS.",
            "..S.............S..",
            "..Ss...........sS..",
            "..sS...........Ss..",
            "...Ss.........sS...",
            "....SS.......SS....",
            ".....SSs...sSS.....",
            "......SSSSSSS......",
            ".......SsSsS.......",
            ".......SdSdS.......",
            ".......SdSdS.......",
            ".......SsSsS......."
        ]
    }
    // Flame crown; dies down while dozing, leaps up on hover.
    PixelSprite {
        objectName: "sauronFlames"
        visible: !art.happy && !art.asleep
        x: 5
        y: 3
        colors: art.colors
        rows: ["...F.F...", "..FfRf.F.", "RfFofFofR"]
    }
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy && !art.asleep
        x: 5
        y: 2
        colors: art.colors
        rows: ["..F...F..", ".Ff.F.fF.", "FofFofFoF"]
    }
    // The burning iris; white-hot while the searchlight charges.
    PixelSprite {
        objectName: "sauronIris"
        visible: !art.shut
        x: 3
        y: 5
        colors: art.colors
        rows: art.charged ? [
            "....rRRRr....",
            "..rRyYYYyRr..",
            ".RyYYYYYYYyR.",
            "RyYYWWWWWYYyR",
            ".RyYYYYYYYyR.",
            "..rRyYYYyRr..",
            "....rRRRr...."
        ] : [
            "....rRRRr....",
            "..rRoyyyoRr..",
            ".RoyYYYYYyoR.",
            "RoyYWWWWWYyoR",
            ".RoyYYYYYyoR.",
            "..rRoyyyoRr..",
            "....rRRRr...."
        ]
    }
    // The slit: follows the pointer; dilates on hover, goes jet black while searching.
    PixelSprite {
        objectName: "companionEye0"
        visible: !art.shut
        x: art.happy ? 8 : 9 + art.pupilX
        y: art.happy ? 6 : 6 + art.pupilY
        colors: art.colors
        rows: art.happy ? ["kxk", "xxx", "xxx", "xxx", "kxk"]
            : art.charged ? ["x", "x", "x", "x", "x"]
            : ["k", "x", "x", "x", "k"]
    }
    // Blink / doze: the eye narrows to a slit (a smouldering one when dozing).
    PixelSprite {
        objectName: "sauronLid"
        visible: art.shut
        x: 3
        y: 7
        colors: art.colors
        rows: ["....rRRRr....", art.asleep ? "rRRoooooooRRr" : "RRoyyyyyyyoRR", "....rRRRr...."]
    }

    // Searchlight: drawn sweeping right, mirrored for a sweep to the left.
    Item {
        objectName: "sauronBeam"
        visible: art.searching && art.habitFrame >= 3
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        PixelSprite {
            x: 13
            y: 6
            colors: art.colors
            rows: art.habitFrame === 5 ? ["....b.", "..b.bb", "bbbbbb", "..b.bb", "....b."]
                : ["....bb", "..bBBB", "BBBBBB", "..bBBB", "....bb"]
        }
    }
}
