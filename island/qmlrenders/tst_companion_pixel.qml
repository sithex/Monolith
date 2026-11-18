import QtQuick
import QtTest
import "../plasma-anchor/contents/ui" as Panel

// Pixel-art companion sheets, driven through the real brain: pointer events for gaze,
// brain state for hover/sleep/blink, and the art's own habit frames.
// Run via render_pixel_companions.py, which sets `outputDirectory` and `characterIds`.
TestCase {
    id: testCase
    readonly property string outputDirectory: "/tmp/companion-pixel"
    readonly property var characterIds: ["companioncube", "nurgling", "hollowknight", "frog"]
    name: "CompanionPixelRender"
    width: 200
    height: 200
    visible: true
    when: windowShown

    Component {
        id: factory
        Rectangle {
            property alias face: face
            width: face.preferredSize
            height: face.preferredSize
            color: "#131313"
            Panel.Companion {
                id: face
                width: preferredSize
                height: preferredSize
                motion: true
                screenArea: Qt.rect(-1e5, -1e5, 2e5, 2e5)
            }
        }
    }

    function point(face, dx, dy) {
        const c = face.mapToGlobal(face.width / 2, face.height / 2);
        face.pointerX = c.x + dx;
        face.pointerY = c.y + dy;
        face.pointerAt = Math.max(Date.now(), face.pointerAt + 1);
        wait(220);
    }

    function test_sheets() {
        for (const id of characterIds) {
            const stage = createTemporaryObject(factory, testCase);
            const face = stage.face;
            face.character = id;
            wait(50);
            const art = face.art;
            function grab(name) {
                wait(30);
                grabImage(stage).save(outputDirectory + "/" + id + "-" + name + ".png");
            }
            for (const p of [["center", 0, 0], ["left", -900, 0], ["right", 900, 0], ["up", 0, -900], ["down", 0, 900], ["near-left", -40, 30], ["near-right", 40, 30], ["downright", 900, 900]]) {
                point(face, p[1], p[2]);
                grab("look-" + p[0]);
            }
            point(face, 0, 0);
            face.blink = 0.08;
            grab("blink");
            face.blink = 1;
            face.happy = true;
            wait(60);
            grab("hover");
            face.happy = false;
            face.sleepy = true;
            face.lid = 0.42;
            wait(60);
            grab("sleep");
            face.sleepy = false;
            face.lid = 1;
            for (const side of [["r", 900], ["l", -900]]) {
                point(face, side[1], 300);
                art.lastHabitAt = 0;
                art.startHabit();
                art.resetHabit();
                for (let f = 1; f <= art.habitFrames; f++) {
                    art.habitFrame = f;
                    grab("habit-" + side[0] + "-" + f);
                }
                art.habitFrame = 0;
            }
            stage.destroy();
        }
    }
}
