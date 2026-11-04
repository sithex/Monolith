import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

// Run through render_companions.py --output. Art states plus real pointer-driven tracking.
TestCase {
    id: testCase
    // render_companions.py --output supplies this property in a temporary harness.
    readonly property string outputDirectory: Qt.resolvedUrl("../../evidence/companion-pack").toString().replace("file://", "")

    function test_render() {
        function capture(stage, id, factor, name) {
            wait(80);
            grabImage(stage).save(outputDirectory + "/renders/" + id + "-" + factor + "x-" + name + ".png");
        }

        for (const id of ["notchmimic", "companioncube", "nurgling", "hollowknight"]) {
            for (const factor of [1, 4]) {
                const stage = createTemporaryObject(factory, testCase, {
                    "factor": factor
                });
                stage.face.character = id;
                const face = stage.face, art = face.art;
                // Disable brain scheduling while testing/capturing the independent art contract.
                face.pointerAt = 1;
                face.shownX = 0;
                face.shownY = 0;
                face.pointerAt = 0;
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
                const centre = face.mapToGlobal(10,10);
                face.pointerX = centre.x;
                face.pointerY = centre.y;
                face.pointerAt = Date.now();
                wait(200);
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

    function test_tracking() {
        for (const factor of [1,4]) {
            const stage = createTemporaryObject(factory, testCase, {factor: factor});
            const face=stage.face;
            face.character="companioncube";
            for (const input of [["center",0,0],["left",-900,0],["right",900,0],["up",0,-900],["down",0,900],["settled",0,0]]) {
                const centre=face.mapToGlobal(10,10);
                face.pointerX=centre.x+input[1];
                face.pointerY=centre.y+input[2];
                face.pointerAt=Math.max(Date.now(),face.pointerAt+1);
                wait(230);
                grabImage(stage).save(outputDirectory+"/renders/cube-track-"+factor+"x-"+input[0]+".png");
            }
            stage.destroy();
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
