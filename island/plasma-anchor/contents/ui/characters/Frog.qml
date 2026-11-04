import QtQuick

// A little frog, as crisp 1× pixel art: big bulging eyes whose pupils follow the pointer,
// a wide smile and a pale belly, sitting at the bottom of the slot.
//
// Hover: happy ^^ eyes and pink cheeks. Idle habit: a fly buzzes in on the side the
// pointer is on, the tongue snaps out and reels it in, then a gulp. Dozing: eyes shut
// and a sleepy bubble swells and pops.
HabitCharacter {
    id: art
    gazeScale: 1
    habitFrames: 9

    readonly property int pupilX: gazePx(gazeX, 0.55, -1, 1)
    readonly property int pupilY: gazePx(gazeY, 0.8, 0, 1)
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool eyesShut: asleep || open < 0.5

    // Which side the fly comes from: chosen when the burst starts.
    property int habitDirection: 1
    onHabitStarted: habitDirection = gazeX < 0 ? -1 : 1
    readonly property bool hunting: !happy && habitFrame > 0
    // 1-2 fly buzzes, 3-4 tongue out, 5-6 reel in, 7-8 gulp, 9 blink.
    readonly property int tongueLength: [0, 0, 0, 3, 6, 3, 1, 0, 0, 0][habitFrame]
    readonly property var tonguePath: [[15, 14], [16, 13], [17, 12], [18, 11], [18, 10], [19, 9]]
    readonly property bool gulping: hunting && (habitFrame === 7 || habitFrame === 8)

    readonly property int frogX: 2
    readonly property int frogY: 7 + (gulping && habitFrame === 7 ? -1 : 0)

    readonly property var colors: ({
        "O": "#173018", "G": "#58b648", "g": "#3c8a36", "L": "#86d264", "B": "#d8e7a0",
        "W": "#f6f6ee", "E": "#101010", "M": "#1d3a1c", "s": "#2f6e2b", "P": "#f08aa0", "T": "#e0546c",
        "f": "#7d8a4a", "w": "#c8d6e0"
    })

    Item {
        objectName: "frogBody"
        x: art.frogX
        y: art.frogY
        width: 16
        height: 13
        PixelSprite {
            colors: art.colors
            rows: [
                "..OOO......OOO..",
                ".OWWWO....OWWWO.",
                "OWWWWWOOOOWWWWWO",
                "OWWWWWGGGGWWWWWO",
                "OGWWWGGLLGGWWWGO",
                "OGGGGGGLLGGGGGGO",
                "OGsGGGGGGGGGGsGO",
                "OGGMGGGGGGGGMGGO",
                art.gulping ? ".OGGMMMMMMMMGGO." : ".OGGMMMMMMMMGGO.",
                art.gulping ? "OGGGGBBBBBBGGGGO" : ".OGGGBBBBBBGGGO.",
                "OGgOBBBBBBBBOgGO",
                "OgGgOBBBBBBOgGgO",
                ".OOOOOOOOOOOOOO."
            ]
        }
        // Pupils: 2×2, sliding inside the eye whites toward the pointer.
        Repeater {
            model: [2, 11]
            PixelSprite {
                required property int modelData
                required property int index
                objectName: "companionEye" + index
                visible: !art.eyesShut && !art.happy
                x: modelData + art.pupilX
                y: 2 + art.pupilY
                colors: art.colors
                rows: ["EE", "EE"]
            }
        }
        // Lids: shut (blink, doze) or happy ^^ on hover.
        Repeater {
            model: [1, 10]
            PixelSprite {
                required property int modelData
                visible: art.eyesShut || art.happy
                x: modelData
                y: 1
                colors: art.colors
                rows: art.happy ? [".GGG.", "GGMGG", "GMGMG", "MGGGM"]
                    : [".GGG.", "GGGGG", "MMMMM", ".GGG."]
            }
        }
        // Blush on hover.
        PixelSprite {
            objectName: "hoverMark"
            visible: art.happy
            x: 1
            y: 6
            colors: art.colors
            rows: ["PP..........PP"]
        }
        // Cheeks puffed while gulping.
        PixelSprite {
            visible: art.gulping
            x: 3
            y: 8
            colors: art.colors
            rows: ["L........L"]
        }
    }

    // Tongue and fly. Drawn for a fly on the right and mirrored for the left.
    Item {
        objectName: "frogHunt"
        visible: art.hunting
        width: 20
        height: 20
        transform: Scale { origin.x: 10; xScale: art.habitDirection < 0 ? -1 : 1 }
        // Tongue: from the corner of the mouth up and out toward the fly.
        Repeater {
            model: art.tongueLength
            Rectangle {
                required property int index
                readonly property var p: art.tonguePath[index]
                x: p[0]
                y: p[1]
                width: 1
                height: 1
                antialiasing: false
                color: index === art.tongueLength - 1 ? "#e0546c" : "#f08aa0"
            }
        }
        PixelSprite {
            objectName: "frogFly"
            readonly property var at: [[0, 0], [16, 2], [18, 5], [18, 7], [0, 0], [16, 10], [14, 12]][Math.min(6, art.habitFrame)]
            visible: art.habitFrame >= 1 && art.habitFrame <= 6 && art.habitFrame !== 4
            x: at[0]
            y: at[1]
            colors: art.colors
            rows: art.habitFrame % 2 ? ["w.", ".f"] : [".w", "f."]
        }
    }

    // Sleepy bubble above the head while dozing (static: nothing animates during sleep).
    PixelSprite {
        objectName: "frogBubble"
        visible: art.asleep
        x: 8
        y: 3
        colors: ({ "b": "#bfe3ff", "c": "#ffffff" })
        rows: [".bb.", "bcbb", "bbbb", ".bb."]
    }
}
