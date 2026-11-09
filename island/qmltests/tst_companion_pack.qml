import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

TestCase {
    id: suite

    property int strayTicks: 0
    readonly property var ids: ["notchmimic", "companioncube", "nurgling", "hollowknight", "frog", "necronwarrior", "necronoverlord", "canoptekscarab", "eyeofsauron", "mushroom", "sonichu", "salamander"]

    function test_pack_data() {
        return ids.map((id) => {
            return ({
                "tag": id,
                "id": id
            });
        });
    }

    function test_pack(data) {
        const face = createTemporaryObject(factory, suite, {
            "character": data.id
        });
        verify(face.art !== null);
        compare(face.shownCharacter, data.id);
        // Pixel characters default to a 20 px slot and may ask for up to 26 px (the 32 px
        // notch's interior is 28 px); the brain sizes the face from the art's request.
        compare(face.preferredSize, face.art.preferredSize);
        verify(face.preferredSize >= 16 && face.preferredSize <= 26, "slot fits the notch: " + face.preferredSize);
        compare(face.art.habitRunning, false);
        const art = face.art;
        const lightEdge = art.edge.toString();
        face.bodyColor = "#171717";
        compare(art.edge.toString(), "#52545b");
        verify(art.edge.toString() !== lightEdge);
        compare(art.habitFrame, 0);
        // Event source is the brain blink; cooldown itself has no ticking timer.
        face.blinked(false);
        compare(art.habitRunning, false);
        const startedAt = Date.now();
        face.blinked(true);
        compare(art.habitRunning, true);
        compare(art.habitFrame, 1);
        let changes = 0;
        const count = function count() {
            changes++;
            compare(art.step, Math.round(art.step));
        };
        art.habitFrameChanged.connect(count);
        tryCompare(art, "habitRunning", false, 1600);
        compare(art.habitFrame, 0);
        const elapsed = Date.now() - startedAt;
        const burst = art.habitFrames * art.habitInterval;
        verify(elapsed >= burst - 100 && elapsed < burst + 250, "burst elapsed " + elapsed);
        verify(changes >= art.habitFrames - 1 && changes <= art.habitFrames);
        face.blinked(true);
        compare(art.habitRunning, false);
        const quietChanges = changes;
        wait(400);
        compare(changes, quietChanges);
        compare(findChild(art, "habitTick").running, false);
        art.habitFrameChanged.disconnect(count);
        for (const gate of ["visible", "motion", "happy", "sleepy", "thinking", "attended", "artVisible"]) {
            art.lastHabitAt = 0;
            art.startHabit();
            verify(art.habitRunning, gate);
            if (gate === "attended")
                face.screenArea = Qt.rect(5000, 5000, 20, 20);
            else if (gate === "artVisible")
                art.visible = false;
            else
                face[gate] = !(gate === "visible" || gate === "motion");
            compare(art.habitRunning, false, gate);
            compare(art.habitFrame, 0, gate);
            wait(150);
            compare(art.habitFrame, 0);
            if (gate === "attended")
                face.screenArea = Qt.rect(-1000, -1000, 2000, 2000);
            else if (gate === "artVisible")
                art.visible = true;
            else
                face[gate] = gate === "visible" || gate === "motion";
        }
        face.pointerX = 900;
        face.pointerY = 900;
        face.pointerAt = Date.now();
        wait(200);
        verify(Math.abs(face.shownX) <= face.maxX * face.preferredSize && Math.abs(face.shownY) <= face.maxY * face.preferredSize);
        for (let i = 0; i < 2; i++) {
            const eye = findChild(face, "companionEye" + i);
            if (eye)
                verify(eye.x >= 0 && eye.y >= 0 && eye.x + eye.width <= face.preferredSize && eye.y + eye.height <= face.preferredSize);

        }
        face.happy = true;
        const mark = findChild(face, "hoverMark");
        verify(mark !== null);
        verify(mark.visible);

        face.happy = false;
        face.sleepy = true;
        compare(art.asleep, true);
        face.sleepy = false;
        art.lastHabitAt = 0;
        art.startHabit();
        strayTicks = 0;
        findChild(art, "habitTick").triggered.connect(function() {
            suite.strayTicks++;
        });
        face.character = "blob";
        compare(face.shownCharacter, "blob");
        wait(300);
        compare(strayTicks, 0);
        face.character = data.id;
        compare(face.shownCharacter, data.id);
        compare(face.art.habitFrame, 0);
        for (const unknown of ["obelisk", "unknown", "null", "constructor", "__proto__", ""]) {
            face.character = unknown;
            compare(face.shownCharacter, "blob");
        }
    }

    function test_layout_data() {
        return test_pack_data();
    }

    function test_layout(data) {
        const pill = createTemporaryObject(pillFactory, suite, {
            "companionCharacter": data.id
        });
        const face = findChild(pill, "notchCompanion");
        const clock = findChild(pill, "panelClock");
        pill.micMuted = true;
        wait(300);
        const mic = findChild(pill, "notchMicMuted");
        verify(face.x + face.width <= mic.x);
        verify(mic.x + mic.width < clock.mapToItem(pill, 0, 0).x);
        pill.width = 124;
        tryCompare(face, "visible", false, 500); // Existing compact-muted rule remains authoritative.
        pill.micMuted = false;
        tryCompare(face, "visible", true, 500);
        verify(face.x + face.width < clock.mapToItem(pill, 0, 0).x);
    }

    name: "CompanionPack"
    width: 200
    height: 180
    visible: true
    when: windowShown

    Component {
        id: factory

        Panel.Companion {
            x: 20
            y: 20
            width: preferredSize
            height: preferredSize
            motion: true
            pointerAt: 1
            screenArea: Qt.rect(-1000, -1000, 2000, 2000)
        }

    }

    Component {
        id: pillFactory

        Panel.PanelPill {
            width: 160
            height: 26
            companion: true
            clockText: "11:59 PM"
        }

    }

}
