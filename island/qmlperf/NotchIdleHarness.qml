import QtQuick
import QtQuick.Window

// Idle-cost harness for the Plasma notch, outside plasmashell. Three windows stand in for
// Nick's three monitors (DP-2 0,360 1920x1080 · DP-1 1920,0 2560x1440 · HDMI-A-1 4480,0
// 2560x1440), each with a PanelPill fed the way main.qml feeds it. The scenario comes from
// the command line as JSON; after `secs` it prints one JSON line with frames per window and
// quits. CPU and wakeups are read from /proc by run_idle_ab.py.
//
//   qml6 NotchIdleHarness.qml -- '<ui dir url>' '{"secs":30,"playing":true,...}'
//
// Scenario keys (all optional): secs, character, companion, playing, working, waiting,
// reactive, waitingPulse, hoverPeek, frozen (game-mode freeze), pointer: "parked"|"moving"|"none",
// pointerScreen (0..2), settleAfterMs (pointer parked this long before measuring starts is
// not simulated: timers are real; use warmSecs), warmSecs (not counted).
Item {
    id: harness
    readonly property var args: Qt.application.arguments
    readonly property string uiDir: args[args.length - 2]
    readonly property var scenario: JSON.parse(args[args.length - 1])
    readonly property var screens: [
        { name: "DP-2", x: 0, y: 360, w: 1920, h: 1080 },
        { name: "DP-1", x: 1920, y: 0, w: 2560, h: 1440 },
        { name: "HDMI-A-1", x: 4480, y: 0, w: 2560, h: 1440 }
    ]
    property var windows: []
    property var pills: []
    property var frames: [0, 0, 0]
    property bool counting: false
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    property int step: 0

    function opt(key, fallback) { return scenario[key] === undefined ? fallback : scenario[key]; }
    function setIf(obj, key, value) { if (key in obj) obj[key] = value; }

    Component {
        id: windowFactory
        Window {
            property int index: 0
            width: 260
            height: 30
            color: "black"
            visible: true
            onFrameSwapped: if (harness.counting) { const f = harness.frames.slice(); f[index]++; harness.frames = f; }
        }
    }

    function feed(pill) {
        pill.pointerX = pointerX;
        pill.pointerY = pointerY;
        pill.pointerAt = pointerAt;
    }

    Component.onCompleted: {
        const comp = Qt.createComponent(uiDir + "/PanelPill.qml");
        if (comp.status !== Component.Ready) { console.error(comp.errorString()); Qt.exit(2); return; }
        const p = opt("pointerScreen", 1);
        const s0 = screens[p];
        // Parked/moving pointer sits in the middle of the chosen monitor.
        pointerX = s0.x + s0.w / 2; pointerY = s0.y + s0.h / 2;
        for (let i = 0; i < 3; i++) {
            const s = screens[i];
            const win = windowFactory.createObject(null, { index: i, x: s.x + s.w / 2 - 130, y: s.y });
            const pill = comp.createObject(win.contentItem, { x: 30, y: 2, width: 200, height: 26,
                clockText: "11:59 PM", dateText: "Thu, Nov 5" });
            setIf(pill, "screenName", s.name);
            const face = findChild(pill, "notchCompanion");
            if (face) face.screenArea = Qt.rect(s.x, s.y, s.w, s.h);
            setIf(pill, "companionCharacter", opt("character", "isaac"));
            setIf(pill, "companion", opt("companion", true));
            setIf(pill, "companionReactive", opt("reactive", true));
            setIf(pill, "waitingPulse", opt("waitingPulse", true));
            setIf(pill, "hoverPeek", opt("hoverPeek", true));
            setIf(pill, "peekTrack", "Song — Artist");
            setIf(pill, "playing", opt("playing", false));
            setIf(pill, "working", opt("working", false));
            setIf(pill, "waiting", opt("waiting", false));
            setIf(pill, "companionFrozen", opt("frozen", false));
            windows.push(win); pills.push(pill);
        }
        if (opt("pointer", "parked") !== "none") {
            pointerAt = Date.now();
            for (const pill of pills) feed(pill);
        }
        warm.start();
    }
    function findChild(item, name) {
        if (!item) return null;
        if (item.objectName === name) return item;
        const kids = item.children || [];
        for (let i = 0; i < kids.length; i++) { const f = findChild(kids[i], name); if (f) return f; }
        return null;
    }
    // A pointer that moves like the KWin relay sends it: 20 Hz while moving, small steps.
    Timer {
        interval: 50
        repeat: true
        running: harness.opt("pointer", "parked") === "moving"
        onTriggered: {
            harness.step++;
            const s = harness.screens[harness.opt("pointerScreen", 1)];
            harness.pointerX = s.x + s.w / 2 + 600 * Math.sin(harness.step / 40);
            harness.pointerY = s.y + s.h / 2 + 300 * Math.cos(harness.step / 55);
            harness.pointerAt = Date.now();
            for (const pill of harness.pills) harness.feed(pill);
        }
    }
    Timer {
        id: warm
        interval: harness.opt("warmSecs", 3) * 1000
        onTriggered: { harness.counting = true; console.info("HARNESS-START"); done.start(); }
    }
    Timer {
        id: done
        interval: harness.opt("secs", 30) * 1000
        onTriggered: {
            harness.counting = false;
            console.info("HARNESS-RESULT " + JSON.stringify({ frames: harness.frames }));
            // Leave the runner time to read /proc before the process goes away.
            quit.start();
        }
    }
    Timer { id: quit; interval: 1500; onTriggered: Qt.quit() }
}
