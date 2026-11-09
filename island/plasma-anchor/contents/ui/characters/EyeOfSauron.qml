import QtQuick

// The Eye of Sauron (The Lord of the Rings), as crisp 1× pixel art (26 px):
// a lidless, horizontally stretched eye wreathed in roaring flame, held between
// the two curved dark-iron horn spires atop Barad-dûr.
//
// The cat-slit pupil and white-hot core track the pointer by whole pixels.
// Hover (hoverMark): the eye flares white-hot, pupil dilates, flames leap high — menacing, not cute.
// Blink: the eye narrows to a fierce horizontal fiery slit.
// Doze: flames die down to dark red and embers, slit smoulders.
// Idle habit: a searchlight sweep — a brilliant cone of white-gold glare flicking
// toward the pointer side and settling back exactly to the rest frame.
HabitCharacter {
    id: art
    slotSize: 26
    gazeScale: 1
    habitFrames: 5
    habitInterval: 125

    readonly property int pupilX: gazePx(gazeX, 1, -2, 2)
    readonly property int pupilY: gazePx(gazeY, 0.7, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool shut: asleep || open < 0.5

    // Directional searchlight habit: sweeps toward the pointer side
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool searching: !happy && habitFrame > 0

    readonly property var colors: ({
        // Barad-dûr dark iron & gothic stone
        "K": "#0c0d12",
        "D": "#181a22",
        "I": "#2c2f3b",
        "L": "#585e70",
        "H": "#9097ac",
        "M": "#74200a",
        "m": "#b03610",

        // Eye of Sauron fire & iris
        "C": "#440602",
        "R": "#8c1204",
        "F": "#ca2c06",
        "O": "#fc5e08",
        "Y": "#ffa812",
        "E": "#ffd83c",
        "W": "#fff698",
        "w": "#ffffff",

        // Cat-slit pupil
        "X": "#040102",
        "P": "#280402",

        // Searchlight beam
        "B": "#fff8b0",
        "b": "#ffc820",
        "a": "#e07e10"
    })

    // Barad-dûr: two curved gothic horn spires cradling the Eye over a tiered tower base.
    PixelSprite {
        objectName: "sauronTower"
        colors: art.colors
        rows: [
            "..........................",
            "..........................",
            "..H....................H..",
            "..LM..................ML..",
            ".LIDm................mDIL.",
            ".LIDm................mDIL.",
            ".LIDm................mDIL.",
            ".LIDm................mDIL.",
            "..LIDm..............mDIL..",
            "..LIDm..............mDIL..",
            "..LIDm..............mDIL..",
            "...LIDm............mDIL...",
            "...LIDm............mDIL...",
            "...LIDm............mDIL...",
            "....LIDm..........mDIL....",
            "....LIID..........DIIL....",
            "....LIIDD........DDIIL....",
            ".....LIDDDm....mDDDIL.....",
            ".....LIDDDKIDDIKDDDIL.....",
            "....LLIIDDKKKKKKDDILL.....",
            "....LIDDDKKKKKKKKDDDIL....",
            "...LLIDDDKKKIIKKKDDDIILL..",
            "...LIDDDKKKIIIIKKKDDDIL...",
            "..LLIDDDKKIIKKIIKKDDDIILL.",
            "..LIDDDKKKIIKKIIKKKDDDIL..",
            "..LIDDDKKKIIKKIIKKKDDDIL.."
        ]
    }

    // Horizontally stretched fiery eye socket and flame wreath.
    PixelSprite {
        objectName: "sauronIris"
        visible: !art.shut
        colors: art.colors
        rows: [
            "..........R....R..........",
            ".......F..O.RR.O..F.......",
            "......FO.FOY..YOF.OF......",
            "......FR.FOYEEEYOF.RF.....",
            ".....CFOOYEWWWWEYOOF......",
            "....CFOYEWYYYYYYWEYOFC....",
            "...CFOYEWYYYYYYYYWEYOFC...",
            "..CFOYEWYYYYYYYYYYWEYOFC..",
            ".CFOOYEWYYYYYYYYYYWEYOFC..",
            ".CFOOYEWYYYYYYYYYYWEYOFC..",
            "..CFOYEWYYYYYYYYYYWEYOFC..",
            "...CFOYEWYYYYYYYYWEYOFC...",
            "....CFOYEWYYYYYYWEYOFC....",
            ".....CFOOYEWWWWEYOOF......",
            "......FR.FOYEEEYOF.RF.....",
            "......FO.FOY..YOF.OF......",
            ".......F..O.CC.O..F.......",
            "..........C....C..........",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            ".........................."
        ]
    }

    // The pupil and white-hot core: tracks pointer by whole pixels;
    // flares wide on hover; darts in habit direction during searchlight sweep.
    PixelSprite {
        objectName: "companionEye0"
        visible: !art.shut
        x: art.happy ? 9 : 9 + (art.searching ? art.habitDirection * (art.habitFrame === 1 ? 1 : art.habitFrame <= 4 ? 2 : 1) : art.pupilX)
        y: art.happy ? 5 : 5 + (art.searching ? 0 : art.pupilY)
        colors: art.colors
        rows: art.happy ? [
            "..WwwW..",
            ".WwXXwW.",
            "WwwXXwwW",
            "wwwXXwww",
            "wwwXXwww",
            "WwwXXwwW",
            ".WwXXwW.",
            "..WwwW.."
        ] : [
            "..EwwE..",
            ".EwXXwE.",
            "EwwXXwwE",
            "WwwXXwwW",
            "WwwXXwwW",
            "EwwXXwwE",
            ".EwXXwE.",
            "..EwwE.."
        ]
    }

    // Hover flare mark: flames leap high to the top edge when hovered.
    PixelSprite {
        objectName: "hoverMark"
        visible: art.happy && !art.asleep
        colors: art.colors
        rows: [
            ".........F..OO..F.........",
            ".......F..O.YY.O..F.......",
            "......F.O.E.WW.E.O.F......",
            ".....FO..Y.EEEE.Y..OF....."
        ]
    }

    // Blink / Doze lid:
    // Narrow fiery slit during blink, dimmed embers when dozing.
    PixelSprite {
        objectName: "sauronLid"
        visible: art.shut
        colors: art.colors
        rows: art.asleep ? [
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            ".....C.RR....RR.C.........",
            "....CFR..RR..RR..RFC......",
            "...CFOO..RR..RR..OOFC.....",
            "..CFOORRRRRRRRRRRROOFC....",
            ".CFOORRROYYYYYYORRROOFC...",
            ".CFOORRROPPYYPPORRROOFC...",
            "..CFOORRRRRRRRRRRROOFC....",
            "...CFOO..RR..RR..OOFC.....",
            "....CFR..RR..RR..RFC......",
            ".....C.RR....RR.C.........",
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
        ] : [
            "..........R....R..........",
            ".......F..O.RR.O..F.......",
            "......FO.FOY..YOF.OF......",
            "......FR.FOYEEEYOF.RF.....",
            ".....CFOOYEWWWWEYOOF......",
            "....CFOOYEFFFFEEYOFC......",
            "...CFOOYEFFFFFFEEYOFC.....",
            "..CFOOYEFOOOOOOFEEYOFC....",
            ".CFOOYEWWwwXXwwWWEYOOFC...",
            ".CFOOYEWWwwXXwwWWEYOOFC...",
            "..CFOOYEFOOOOOOFEEYOFC....",
            "...CFOOYEFFFFFFEEYOFC.....",
            "....CFOOYEFFFFEEYOFC......",
            ".....CFOOYEWWWWEYOOF......",
            "......FR.FOYEEEYOF.RF.....",
            "......FO.FOY..YOF.OF......",
            ".......F..O.CC.O..F.......",
            "..........C....C..........",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            "..........................",
            ".........................."
        ]
    }

    // Searchlight beam: conical beam of brilliant light sweeping across the side
    PixelSprite {
        objectName: "sauronBeam"
        visible: art.searching && art.habitFrame >= 2 && art.habitFrame <= 4
        colors: art.colors
        rows: {
            if (art.habitDirection > 0) {
                if (art.habitFrame === 3) {
                    return [
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        ".................aaaaaaa..",
                        "................bbbbbbbbbb",
                        "...............BBBBBBBBBBB",
                        "...............BBBBBBBBBBB",
                        "................bbbbbbbbbb",
                        ".................aaaaaaa.."
                    ];
                } else {
                    return [
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "................aaaaaa....",
                        "...............bbbbbbbbbbb",
                        "...............bbbbbbbbbbb",
                        "................aaaaaa...."
                    ];
                }
            } else {
                if (art.habitFrame === 3) {
                    return [
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..aaaaaaa.................",
                        "bbbbbbbbbb................",
                        "BBBBBBBBBBB...............",
                        "BBBBBBBBBBB...............",
                        "bbbbbbbbbb................",
                        "..aaaaaaa................."
                    ];
                } else {
                    return [
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "..........................",
                        "....aaaaaa................",
                        "bbbbbbbbbbb...............",
                        "bbbbbbbbbbb...............",
                        "....aaaaaa................"
                    ];
                }
            }
        }
    }
}
