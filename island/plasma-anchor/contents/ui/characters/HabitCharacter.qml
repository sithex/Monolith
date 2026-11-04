import QtQuick
import QtQuick.Window

// Shared, event-driven burst. No timer runs between habits (including cooldown).
Item {
    id: art

    property Item face
    readonly property real preferredSize: 20
    readonly property real gazeScale: 0.45
    readonly property bool happy: face ? face.happy : false
    readonly property bool asleep: face ? face.sleepy : false
    readonly property color edge: face && face.bodyColor.hslLightness < 0.5 ? "#52545b" : "#85858b"
    readonly property bool allowed: !!face && visible && face.visible && face.attended && face.motion && !face.happy && !face.sleepy && !face.thinking
    property int habitFrame: 0
    property double lastHabitAt: 0
    readonly property bool habitRunning: tick.running
    readonly property int step: [0, 1, 2, 1, 2, 1, 0][habitFrame]
    readonly property real pixel: 1 / Math.max(1, Screen.devicePixelRatio)

    function snap(v) {
        return Math.round(v / pixel) * pixel;
    }

    function resetHabit() {
        tick.stop();
        habitFrame = 0;
    }

    function startHabit() {
        if (!allowed || tick.running || Date.now() - lastHabitAt < 12000)
            return ;

        lastHabitAt = Date.now();
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
        interval: 125
        repeat: true
        onTriggered: {
            if (!art.allowed || art.habitFrame >= 6)
                art.resetHabit();
            else
                art.habitFrame++;
        }
    }

}
