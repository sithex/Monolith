import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

// Run from repository root. Deterministic art-only capture; production brain tested separately.
TestCase {
    id: testCase

    function test_render() {
        function capture(stage, id, factor, name) {
            wait(80);
            grabImage(stage).save(Qt.resolvedUrl("../../evidence/companion-pack/renders/").toString().replace("file://", "") + id + "-" + factor + "x-" + name + ".png");
        }

        for (const id of ["notchmimic", "obelisk", "companioncube", "nurgling", "hollowknight"]) {
            for (const factor of [1, 4]) {
                const stage = createTemporaryObject(factory, testCase, {
                    "factor": factor
                });
                stage.face.character = id;
                const face = stage.face, art = face.art;
                // Disable brain scheduling while testing/capturing the independent art contract.
                face.pointerAt = 0;
                face.shownX = 0;
                face.shownY = 0;
                face.blink = 1;
                face.lid = 1;
                wait(250);
                capture(stage, id, factor, "normal");
                face.happy = true;
                capture(stage, id, factor, "hover");
                face.happy = false;
                face.sleepy = true;
                face.lid = 0.42;
                wait(650);
                capture(stage, id, factor, "sleep");
                face.sleepy = false;
                face.lid = 1;
                wait(650);
                face.shownX = 0;
                face.shownY = 0;
                face.blink = 1;
                wait(200);
                // Freeze a reproducible habit pose; timing/cleanup exercised by CompanionPack.
                art.habitFrame = 2;
                capture(stage, id, factor, "habit");
                art.habitFrame = 0;
                capture(stage, id, factor, "settled");
                face.pointerX = face.mapToGlobal(10, 10).x;
                face.pointerY = face.mapToGlobal(10, 10).y;
                face.pointerAt = Date.now();
                wait(200);
                face.shownX = 0;
                face.shownY = 0;
                capture(stage, id, factor, "timed-before");
                art.lastHabitAt = 0;
                art.startHabit();
                verify(art.habitRunning);
                for (let frame = 1; frame <= 6; frame++) {
                    capture(stage, id, factor, "timed-" + frame);
                    wait(40);
                }
                tryCompare(art, "habitRunning", false, 1100);
                capture(stage, id, factor, "timed-after");
                stage.destroy();
            }
        }
    }

    name: "CompanionRender"
    width: 160
    height: 160
    visible: true
    when: windowShown

    Component {
        id: factory

        Rectangle {
            property int factor: 1
            property alias face: face

            width: 20 * factor
            height: 20 * factor
            color: "#121215"

            Panel.Companion {
                id: face

                width: 20
                height: 20
                scale: parent.factor
                transformOrigin: Item.TopLeft
                pointerAt: 1
                pointerX: 0
                pointerY: 0
                screenArea: Qt.rect(-1000, -1000, 2000, 2000)
            }

        }

    }

}
