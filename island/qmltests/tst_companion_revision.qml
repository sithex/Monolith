import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

// Pixel-art companions: real pointer input moves
// the artwork, all drawing stays on whole pixels inside the 20×20 slot in every state,
// sub-pixel brain jitter draws nothing, and the hover/habit/sleep looks appear.
TestCase {
    id: suite
    name: "CompanionPixel"
    width: 220; height: 220; visible: true; when: windowShown
    readonly property var ids: ["companioncube", "nurgling", "hollowknight", "frog", "necronwarrior", "necronoverlord", "canoptekscarab", "eyeofsauron", "mushroom", "sonichu", "salamander"]

    Component {
        id: factory
        Panel.Companion {
            x: 80; y: 80; width: 20; height: 20
            motion: true
            screenArea: Qt.rect(-2000, -2000, 4000, 4000)
        }
    }
    function point(face, dx, dy) {
        const c = face.mapToGlobal(10, 10);
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
            verify(r.x0 >= 0 && r.y0 >= 0 && r.x1 <= 20 && r.y1 <= 20, what + ": inside slot " + JSON.stringify(r));
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
        for (const id of ["hollowknight", "frog", "necronwarrior", "eyeofsauron", "mushroom", "sonichu", "salamander"]) {
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
