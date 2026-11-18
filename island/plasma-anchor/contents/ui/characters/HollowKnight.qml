import QtQuick

// The Knight: ivory shell over a ragged slate cloak. The black sockets move as
// whole shapes inside the mask, with a pinprick of reflected light in each.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 6

    readonly property int eyeX: gazePx(gazeX, 0.6, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.7, -1, 1)
    readonly property int lean: gazePx(gazeX, 0.42, -1, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool shut: open < 0.3
    readonly property bool heavyLids: asleep || open < 0.7
    readonly property bool ready: happy && !asleep

    // Draw, sweep, follow through, fade. The last two steps hold the settled pose.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool slashing: !happy && !asleep && habitFrame >= 1 && habitFrame <= 4
    readonly property int recoil: slashing ? [0, 2, 3, 2, 1][habitFrame] * habitDirection : 0

    readonly property var shellColors: ({
        "K": "#23232d", "W": "#fffdf8", "M": "#f4f1ea", "P": "#dedbe4", "S": "#aaa6b7"
    })
    readonly property var cloakColors: ({
        "D": "#1c1e2b", "F": "#343a50", "C": "#4e5469", "L": "#68738d"
    })
    readonly property var eyeColors: ({ "E": "#07070b", "R": "#504d62", "H": "#ffffff" })
    readonly property var nailColors: ({
        "g": "#62515a", "n": "#858399", "N": "#d0cddc", "A": "#f2f5ff",
        "a": "#aab9e3", "f": "#627292"
    })

    PixelSprite {
        objectName: "knightCloak"
        x: 5 - art.recoil
        y: 16
        colors: art.cloakColors
        rows: [
            ".....DDDDDD.....",
            "...DDLLCCCCDD...",
            "..DCLCCCFCLCFD..",
            "..DLCCFCLCCFFFD.",
            ".DLCCFFLCCFFFD..",
            ".DCCDDCCFDFFD...",
            "..DD..DDDDDD....",
            "....FC...FC.....",
            "....DF...DF....."
        ]
    }

    PixelSprite {
        visible: art.ready
        x: 18
        y: 17
        colors: art.cloakColors
        rows: ["...DD", ".DCLF", "DCCDD", ".DD"]
    }
    PixelSprite {
        objectName: "hoverMark"
        visible: art.ready
        x: 20
        y: 7
        colors: art.nailColors
        rows: ["...A", "..AN", "..AN", "..AN", "..AN", "..AN", "..AN", "..An", "..An", ".nNn", "..g", "..g"]
    }

    PixelSprite {
        objectName: "knightSlash"
        visible: art.slashing
        readonly property var poses: [
            [],
            ["....A", "...AN", "...An", "..AN", "..An", ".AN", ".An", "nNn", ".g", "g"],
            ["....fa", "......aA", "........Aa", ".........A", ".........A", "n........a",
             "gNNNAAAafa", "n........A", "........Aa", ".......Aa", ".....af", "....f"],
            ["...fa", ".....aA", ".......A", "........Aa", ".........a", ".........f",
             ".........f", "n.......f", "gNn....af", "nNn..af", "..ANa", "...An", "....A"],
            ["...f", "", ".....a", "", "......f", "", "......a", "", "......f", "", "....a", "", "...f"]
        ]
        readonly property bool facesLeft: art.habitDirection < 0
        readonly property int rightX: art.habitFrame === 2 || art.habitFrame === 3 ? 16 : 18
        readonly property int span: rightX === 16 ? 10 : 8
        x: facesLeft ? art.slotSize - rightX - span : rightX
        y: art.habitFrame === 1 ? 8 : 10
        colors: art.nailColors
        // Pad before flipping so the short rows stay aligned with the same pixel grid.
        rows: !art.slashing ? [] : facesLeft
            ? poses[art.habitFrame].map(row => row.padEnd(span, ".").split("").reverse().join(""))
            : poses[art.habitFrame]
    }

    Item {
        objectName: "knightHead"
        x: 4 + (art.slashing ? -art.recoil : art.ready || art.asleep ? 0 : art.lean)
        y: art.asleep || (art.slashing && art.habitFrame === 1) ? 2 : 1
        width: 18
        height: 18
        PixelSprite {
            colors: art.shellColors
            rows: [
                "....WK......KW....",
                "...WM........MW...",
                "..WMK........KMW..",
                ".KWM..........MWK.",
                ".KWM..........MPK.",
                ".KWMP........MSPK.",
                "..KWMS......MSPK..",
                "..KWMMSKKKKMPPSK..",
                ".KWMMMWMMMMMMPPSK.",
                "KWWMMMMMMMMMMMPPSK",
                "KWMMMMMMMMMMMMMPSK",
                "KMMMMMMMMMMMMMMPSK",
                "KMMMMMMMMMMMMMMPSK",
                "KPMMMMMMMMMMMMMPSK",
                ".KPMMMMMMMMMMMPSK.",
                "..KPPMMMMMMMPPSK..",
                "...KSSPPPPSSSK....",
                ".....KKKKKKK......"
            ]
        }

        Repeater {
            model: [3, 10]
            PixelSprite {
                required property int index
                required property int modelData
                objectName: "companionEye" + index
                x: modelData + (art.asleep ? 0 : art.slashing ? art.habitDirection : art.eyeX)
                y: (art.shut || art.heavyLids ? 12 : 9) + (art.asleep || art.slashing ? 0 : art.eyeY)
                colors: art.eyeColors
                rows: art.shut ? [".REE."] : art.heavyLids ? [".RRR.", "REEER", ".EEE."]
                    : [".REE.", "RHEEE", "EEEEE", "EEEEE", ".EEE."]
            }
        }
    }
}
