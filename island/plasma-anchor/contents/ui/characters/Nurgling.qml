import QtQuick

// A 26 px Nurgling: a crooked horn, crooked grin and a luminous, lumpy paunch.
// Hover sticks out its tongue; idle giggles shake it as a fly circles the horn.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 8
    habitInterval: 110

    readonly property int eyeX: gazePx(gazeX, 0.5, -1, 1)
    readonly property int eyeY: gazePx(gazeY, 0.6, -1, 1)
    readonly property bool giggling: habitFrame > 0
    readonly property var laughSteps: [0, -1, 1, -1, 1, -1, 1, -1, 0]
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool squint: happy || giggling
    readonly property bool shut: asleep || open < 0.5

    readonly property var colors: ({
        "K": "#242b1a", "d": "#48582d", "g": "#768943", "G": "#a8b65b", "L": "#ced378", "H": "#e9e59d",
        "J": "#675839", "B": "#ad9260", "b": "#d8bc82", "I": "#f3dda4", "T": "#fff0ce", "t": "#cfb584",
        "X": "#201a17", "M": "#43212a", "r": "#8c4352", "p": "#cb6977", "P": "#ef9c98", "Y": "#f7dd75",
        "E": "#191b12", "W": "#ffffff", "F": "#85926c", "w": "#c9dee1"
    })

    Item {
        objectName: "nurglingBody"
        x: art.laughSteps[art.habitFrame]
        width: art.slotSize
        height: art.slotSize
        PixelSprite {
            colors: art.colors
            rows: [
                "..........................",
                "..................J.......",
                "................JIJ.......",
                "...............JIbK.......",
                ".........KKKKKIbbK........",
                ".......KKLLGJbbJKKK.......",
                "......KLLHLLGBJGGggK......",
                ".....KLLLGGGGLGddGGggK....",
                "....KLGGddGGGGddGGggdK....",
                "....KLGGGGGGLGGGGGGgpgK...",
                "...KGGGGGGGLLGGGGGGggddK..",
                "...KGGrGGGGGddGGGGGGMdgK..",
                "...KGLMGGGGGGgGGGGMXXdgK..",
                "...KGLMXMGGGGGGMMTtXMdK...",
                "..KgGLrXTMTTXTTXtXXrdgK...",
                "..KLGGGrXtXXXXXXTXMgdgK...",
                ".KgLGdGGrXrtTrpMMgGgdGgKK.",
                ".KLgGKGLGGrMMMrrgGGLgKGLK.",
                "..KtKKGLHLLLLLGGGGGgdKtKK.",
                "....KGLHLLYpLLLLGGGGgdKK..",
                "....KGLHLLPprLLLGdGppgdK..",
                "....KgGLLLLrrLLGgXgYrGdK..",
                "....KgGGLLLLLLGGGdggGddK..",
                ".....KdgGGGGGGGGGGggddK...",
                "....KtGtKddggggdddKgGtgK..",
                "....KKKKK.KKKKKKK.KKKKKK.."
            ]
        }
        // Fixed, staggered sockets; only the two-pixel pupils move with the gaze.
        Repeater {
            model: [6, 15]
            PixelSprite {
                required property int modelData
                required property int index
                x: modelData
                y: 8 - index
                colors: art.colors
                rows: art.shut ? ["....", ".dd.", "dEEd"]
                    : art.squint ? ["....", ".EE.", "E..E"]
                    : [".dd.", "dLLd", "dYYd", ".gg."]

                PixelSprite {
                    objectName: "companionEye" + parent.index
                    visible: !art.squint && !art.shut
                    x: 1 + art.eyeX
                    y: 1 + art.eyeY
                    colors: art.colors
                    rows: ["EW", "EE"]
                }
            }
        }
        // Tongue, poked out on hover.
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy && !art.asleep
            x: 12
            y: 15
            colors: art.colors
            rows: [".Mr.", "MrpM", "MrPM", ".rpM", "..M."]
        }
    }

    // One of Nurgle's flies: circles the horn while giggling, sits on it while dozing.
    readonly property var flyPath: [[0, 0], [13, 1], [16, 0], [20, 1], [21, 3], [20, 5], [16, 4], [13, 3], [15, 1]]
    PixelSprite {
        objectName: "nurglingFly"
        readonly property var at: art.asleep ? [18, 0] : art.flyPath[art.habitFrame]
        visible: art.asleep || (art.giggling && !art.happy)
        x: at[0]
        y: at[1]
        colors: art.colors
        rows: art.asleep ? [".w.", "wEF"]
            : art.habitFrame % 2 ? ["w.w", ".EF"] : [".w.", "FEw"]
    }
}
