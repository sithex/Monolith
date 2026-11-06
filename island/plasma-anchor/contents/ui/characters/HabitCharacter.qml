import QtQuick
import QtQuick.Window

// Shared, event-driven burst. No timer runs between habits (including cooldown).
// A resting idle blink from the brain starts `habitFrames` steps of `habitInterval` ms;
// the art draws itself from `habitFrame` (0 = settled). Hover, sleep, thinking, reduced
// motion, hidden art or another monitor cancel it and snap back to frame 0.
Item {
    id: art

    property Item face
    readonly property real preferredSize: 20
    // How far the brain moves the gaze (see Companion.retarget). Pixel-art characters set
    // 1 and quantise it themselves with `gazePx`.
    property real gazeScale: 0.45
    readonly property bool happy: face ? face.happy : false
    readonly property bool asleep: face ? face.sleepy : false
    // idle | working | waiting | sleeping (Companion.mood); a still face while Hermes waits.
    readonly property string mood: face ? face.mood : "idle"
    readonly property color edge: face && face.bodyColor.hslLightness < 0.5 ? "#52545b" : "#85858b"
    readonly property bool allowed: !!face && visible && face.visible && face.attended && face.motion && !face.happy && !face.sleepy && !face.thinking && !face.frozen && mood !== "waiting"
    property int habitFrames: 6
    property int habitInterval: 125
    property int habitCooldown: 12000
    property int habitFrame: 0
    property double lastHabitAt: 0
    readonly property bool habitRunning: tick.running
    readonly property int step: [0, 1, 2, 1, 2, 1, 0][Math.min(6, habitFrame)]
    readonly property real pixel: 1 / Math.max(1, Screen.devicePixelRatio)
    // Gaze in whole sprite pixels: the brain's quarter-pixel offset times `k`, rounded and
    // clamped. A brain update that doesn't cross a whole pixel leaves these unchanged, so
    // nothing redraws.
    readonly property real gazeX: face ? face.shownX : 0
    readonly property real gazeY: face ? face.shownY : 0
    function gazePx(v, k, lo, hi) {
        const r = Math.round(v * k);
        return r < lo ? lo : r > hi ? hi : (r === 0 ? 0 : r);
    }

    function snap(v) {
        return Math.round(v / pixel) * pixel;
    }

    function resetHabit() {
        tick.stop();
        habitFrame = 0;
    }

    // Hook for characters that pick a direction etc. when a burst begins.
    signal habitStarted()

    function startHabit() {
        if (!allowed || tick.running || Date.now() - lastHabitAt < habitCooldown)
            return ;

        lastHabitAt = Date.now();
        habitStarted();
        habitFrame = 1;
        tick.start();
    }

    onAllowedChanged: {
        if (!allowed) {
            resetHabit();
        }
    }
    Component.onDestruction: resetHabit()

    Connections {
        function onBlinked(resting) {
            if (resting)
                art.startHabit();

        }

        target: art.face
    }

    Timer {
        id: tick

        objectName: "habitTick"
        interval: art.habitInterval
        repeat: true
        onTriggered: {
            if (!art.allowed || art.habitFrame >= art.habitFrames)
                art.resetHabit();
            else
                art.habitFrame++;
        }
    }

}
