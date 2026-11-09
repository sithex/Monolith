import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

// Pixel-art companions: real pointer input moves
// the artwork, all drawing stays on whole pixels inside the character's own square slot (20–26 px) in every state,
// sub-pixel brain jitter draws nothing, and the hover/habit/sleep looks appear.
TestCase {
    id: suite
    name: "CompanionPixel"
    width: 220; height: 220; visible: true; when: windowShown
    readonly property var ids: ["companioncube", "nurgling", "hollowknight", "frog", "necronwarrior", "necronoverlord", "canoptekscarab", "eyeofsauron", "mushroom", "sonichu", "salamander"]

    Component {
        id: factory
        Panel.Companion {
            x: 80; y: 80; width: preferredSize; height: preferredSize
            motion: true
            screenArea: Qt.rect(-2000, -2000, 4000, 4000)
        }
    }
    function point(face, dx, dy) {
        const c = face.mapToGlobal(face.width / 2, face.height / 2);
        face.pointerX = c.x + dx;
        face.pointerY = c.y + dy;
        face.pointerAt = Math.max(Date.now(), face.pointerAt + 1);
        wait(210);
    }
    // Every drawn pixel run, in face coordinates.
    function runs(item, face, out) {
        if (!item.visible) return out;
        if (item instanceof Rectangle && item.width > 0 && item.height > 0) {
            const a = item.mapToItem(face, 0, 0), b = item.mapToItem(face, item.width, item.height);
            out.push({ x0: Math.min(a.x, b.x), y0: Math.min(a.y, b.y), x1: Math.max(a.x, b.x), y1: Math.max(a.y, b.y), aa: item.antialiasing, c: item.color.toString() });
        }
        for (let i = 0; i < item.children.length; i++) runs(item.children[i], face, out);
        return out;
    }
    // A signature of what's drawn: changes iff the picture changes.
    function picture(face) {
        // Let a brain blink or side glance finish first: they're not the thing under test.
        tryCompare(face, "blink", 1, 2000);
        tryCompare(face, "glance", 0, 2000);
        return JSON.stringify(runs(face.art, face, []).map(r => [r.x0, r.y0, r.x1, r.y1, r.c]));
    }
    function checkBounds(face, what) {
        for (const r of runs(face.art, face, [])) {
            verify(!r.aa, what + ": pixel art must not antialias");
            for (const v of [r.x0, r.y0, r.x1, r.y1]) compare(v, Math.round(v), what + ": whole pixels " + JSON.stringify(r));
            const s = face.preferredSize;
            verify(r.x0 >= 0 && r.y0 >= 0 && r.x1 <= s && r.y1 <= s, what + ": inside slot " + JSON.stringify(r));
        }
    }

    function test_tracking_data() { return ids.map(id => ({ tag: id, id: id })); }
    function test_tracking(data) {
        const f = createTemporaryObject(factory, suite, { character: data.id });
        point(f, 0, 0);
        const centre = picture(f);
        const seen = {};
        for (const p of [["left", -900, 0], ["right", 900, 0], ["down", 0, 900]]) {
            point(f, p[1], p[2]);
            seen[p[0]] = picture(f);
            verify(seen[p[0]] !== centre, data.id + " must visibly respond to pointer " + p[0]);
        }
        verify(seen.left !== seen.right, data.id + " left and right poses differ");
        // Sub-pixel jitter with fresh timestamps draws nothing new.
        point(f, 0, 0);
        const still = picture(f);
        for (let i = 0; i < 10; i++) {
            point(f, (i % 2) * 0.3, 0.3);
            compare(picture(f), still, data.id + " jitter must not redraw");
        }
        // Another monitor: holds the pose.
        point(f, 900, 0);
        const held = picture(f);
        f.screenArea = Qt.rect(5000, 5000, 10, 10);
        point(f, -900, 900);
        compare(picture(f), held, data.id + " holds still when unattended");
    }

    function test_states_data() { return ids.map(id => ({ tag: id, id: id })); }
    function test_states(data) {
        const f = createTemporaryObject(factory, suite, { character: data.id }), a = f.art;
        compare(a.gazeScale, 1);
        for (const p of [[-900, -900], [0, 0], [900, 900], [-900, 300], [900, -300], [40, 30]]) {
            point(f, p[0], p[1]);
            a.lastHabitAt = 0; a.startHabit(); a.resetHabit();
            for (let fr = 0; fr <= a.habitFrames; fr++) { a.habitFrame = fr; checkBounds(f, data.id + " habit " + fr); }
            a.habitFrame = 0;
            f.happy = true; checkBounds(f, data.id + " hover"); f.happy = false;
            f.blink = 0.08; checkBounds(f, data.id + " blink"); f.blink = 1;
        }
        point(f, 0, 0);
        const normal = picture(f);
        f.happy = true;
        verify(findChild(a, "hoverMark").visible, "hover mark");
        verify(picture(f) !== normal, "hover looks different");
        f.happy = false;
        f.sleepy = true; f.lid = 0.42;
        checkBounds(f, data.id + " asleep");
        verify(picture(f) !== normal, "dozing looks different");
        f.sleepy = false; f.lid = 1;
        point(f, 0, 0);
        // A real burst from the brain's idle blink changes the drawing and settles back.
        a.lastHabitAt = 0;
        f.blinked(true);
        verify(a.habitRunning);
        let changed = false;
        for (let i = 0; i < a.habitFrames; i++) { wait(125); if (picture(f) !== normal) changed = true; }
        verify(changed, data.id + " habit is visible");
        tryCompare(a, "habitRunning", false, 1600);
        const after = picture(f);
        compare(after, normal, data.id + " settles back");
    }

    function test_directional_habits() {
        // Mushroom's reference bob is vertical, not the old pointer-directed spore puff.
        for (const id of ["hollowknight", "frog", "necronwarrior", "eyeofsauron", "sonichu", "salamander"]) {
            const f = createTemporaryObject(factory, suite, { character: id }), a = f.art;
            point(f, 900, 200);
            a.lastHabitAt = 0; a.startHabit();
            compare(a.habitDirection, 1, id + " toward the pointer (right)");
            a.resetHabit();
            point(f, -900, 200);
            a.lastHabitAt = 0; a.startHabit();
            compare(a.habitDirection, -1, id + " toward the pointer (left)");
            a.resetHabit();
        }
    }

    function test_mushroom_reference_bob() {
        const f = createTemporaryObject(factory, suite, { character: "mushroom" }), a = f.art;
        point(f, 0, 0);
        compare(f.preferredSize, 26);
        compare(a.habitFrames, 8);
        compare(a.habitInterval, 200);
        const cap = findChild(a, "mushroomCap"), body = findChild(a, "mushroomBody");
        const skin = findChild(a, "mushroomSkin"), eye = findChild(a, "companionEye0");
        const settled = picture(f);
        compare(JSON.stringify(eye.rows), JSON.stringify(["EH", "EE", "EE"]));
        point(f, 0, -900);
        const up = picture(f);
        point(f, 0, 900);
        verify(picture(f) !== up, "mushroom up and down gaze differ");
        point(f, 0, 0);
        a.habitFrame = 1;
        compare(cap.y, 1); compare(body.y, 17);
        compare(skin.rows.length, 9);
        a.habitFrame = 2;
        compare(cap.y, 1); compare(body.y, 17);
        compare(skin.rows.length, 8);
        compare(skin.rows[1].indexOf("K"), skin.settledRows[1].indexOf("K") - 1);
        compare(skin.rows[1].lastIndexOf("K"), skin.settledRows[1].lastIndexOf("K") + 1);
        a.habitFrame = 4;
        compare(cap.y, 0); compare(body.y, 16);
        compare(picture(f), settled, "spring returns to approved settled pixels");
        a.habitFrame = 8;
        compare(JSON.stringify(eye.rows), JSON.stringify(["EE"]), "reference last-frame blink");
        a.resetHabit();
        compare(picture(f), settled, "bob settles exactly to frame zero");
        f.blink = 0.08;
        compare(JSON.stringify(eye.rows), JSON.stringify(["EE"]), "brain blink is a two-pixel line");
        f.blink = 1;
        f.sleepy = true; f.lid = 0.42;
        compare(JSON.stringify(eye.rows), JSON.stringify(["bb", "EE", "EE"]), "half-closed doze lids");
    }

    function test_salamander_controller_concept() {
        const f = createTemporaryObject(factory, suite, { character: "salamander" }), a = f.art;
        const blank = "..........................";
        const expected = Array(8).fill(blank).concat([
            "....................hhh...",
            "...................hEEHh..",
            "..........hhhhhh..hBEEEYh.",
            "Y........hBYYBBBhhYYBBBBBh",
            "Bh......hBYYBBBBBYYYBBBBBB",
            "bBh...hhBBBBBBYYBBBBBBBKKb",
            ".bBhhhBBYYBBBBYYBBBBBbbbb.",
            "..bbBBBBYYBBBBBBBBBbb.....",
            "....bbbbbBBb...bbBBb......",
            "........bBBb.....bBBb.....",
            "......bbb.bb.....bbb.b...."
        ], Array(7).fill(blank));
        point(f, 900, 0);
        compare(JSON.stringify(a.currentBodyRows), JSON.stringify(expected), "approved right pose, pixel for pixel");
        const palette = { K: "#0f1219", B: "#323c52", b: "#262e40", h: "#56637f",
            Y: "#ffc81e", O: "#ff8f1f", E: "#06070a", H: "#ffffff" };
        for (const key of Object.keys(palette)) compare(a.colors[key], palette[key]);
        point(f, -900, 0);
        compare(JSON.stringify(a.currentBodyRows), JSON.stringify(expected.map(row => row.split("").reverse().join(""))), "exact left mirror");
        point(f, 900, -900);
        compare(a.currentBodyRows[8][22], "H", "raised eye and glint");
        point(f, 900, 900);
        compare(a.currentBodyRows[11][22], "H", "glint follows downward pointer within lowered eye");
        f.blink = 0.08;
        verify(a.currentBodyRows.join("").indexOf("H") < 0, "blink closes eye");
        f.blink = 1;
        f.sleepy = true; f.lid = 0.42;
        verify(a.dozing, "sleep branch is reachable with lowered lids");
        verify(a.currentBodyRows.join("").indexOf("H") < 0, "sleep has no eye glint");
        compare(a.currentBodyRows[11][0], ".", "tail coils inward");
        checkBounds(f, "controller concept sleep");
    }

    function test_facing_retention() {
        // Salamander keeps its last horizontal facing when gazeX is near centre.
        const f = createTemporaryObject(factory, suite, { character: "salamander" });
        point(f, 900, 0);
        point(f, 0, 0);
        const centerFacingRight = picture(f);
        point(f, -900, 0);
        point(f, 0, 0);
        const centerFacingLeft = picture(f);
        verify(centerFacingRight !== centerFacingLeft, "salamander retains facing when pointer returns to center");
    }

    // A gaze target reversed mid-glide must end at the new target (it used to compare
    // with the half-animated value, skip the update and finish the stale glide).
    function test_reversed_glide_settles() {
        const f = createTemporaryObject(factory, suite, { character: "frog" });
        point(f, 0, 0);
        compare(f.shownY, 0);
        f.sleepy = true;
        f.sleepy = false;
        wait(300);
        compare(f.shownY, 0, "eyes return from a cancelled doze look");
        f.glance = 0.09;
        f.glance = 0;
        wait(300);
        compare(f.shownX, 0, "eyes return from a cancelled glance");
    }

    function test_pixel_sprite() {
        const s = createTemporaryObject(Qt.createComponent("../plasma-anchor/contents/ui/characters/PixelSprite.qml"), suite,
            { rows: ["aab.", "..bb"], colors: { a: "#ff0000", b: "#00ff00" } });
        compare(s.width, 4); compare(s.height, 2);
        compare(s.runs.length, 3);
        compare(JSON.stringify(s.runs.map(r => [r.x, r.y, r.w])), JSON.stringify([[0, 0, 2], [2, 0, 1], [2, 1, 2]]));
        // Unknown keys never index the prototype.
        s.rows = ["c"]; s.colors = { a: "#fff" };
        compare(s.runs.length, 0);
        s.rows = ["constructor"]; compare(s.runs.length, 0);
        // A rebinding that draws the same picture keeps the existing runs (no rebuild).
        s.rows = ["aab.", "..bb"]; s.colors = { a: "#ff0000", b: "#00ff00" };
        const kept = s.runs;
        s.rows = ["aab.", "..bb"];
        verify(s.runs === kept, "same pixels must not rebuild the sprite");
        s.rows = ["aaa.", "..bb"];
        verify(s.runs !== kept);
    }
}
