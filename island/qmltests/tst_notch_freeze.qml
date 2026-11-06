import QtQuick
import QtTest
import QtQuick.Window
import "../plasma-anchor/contents/ui" as Panel

// Game-mode freeze (Settings → gameModeFreezeCompanion, delivered to the notch as
// `companionFrozen`) and the idle-cost rules behind the single stepped clock.
TestCase {
    id: testCase
    name: "NotchFreeze"
    width: 400
    height: 140
    visible: true
    when: windowShown
    Component {
        id: factory
        Panel.PanelPill { x: 100; y: 50; width: 144; height: 26; clockText: "11:59 PM"; dateText: "Sun, Nov 1" }
    }
    readonly property var characters: ["blob", "isaac", "notchmimic", "companioncube", "nurgling", "hollowknight", "frog"]

    // Every Timer / animation under `item` (and `item` itself) that is running right now.
    function runningThings(item, found) {
        found = found || [];
        if (!item) return found;
        if (typeof item.running === "boolean" && item.running) found.push(String(item.objectName || item));
        const kids = item.data || [];
        for (let i = 0; i < kids.length; i++) runningThings(kids[i], found);
        return found;
    }
    function lively(character) {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        notch.companionCharacter = character;
        notch.playing = true;
        notch.working = true;
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x + 300; notch.pointerY = centre.y + 40; notch.pointerAt = Date.now();
        verify(face.attended, character);
        return { notch: notch, face: face, centre: centre };
    }

    function test_freeze_leaves_no_timer_or_animation_running_for_any_character() {
        for (const name of characters) {
            const c = lively(name);
            const stepper = findChild(c.notch, "notchStepper");
            tryVerify(function() { return stepper.running; }, 1000, name);
            verify(runningThings(c.notch).length > 0, name + ": something runs while alive");
            c.notch.companionFrozen = true;
            const left = runningThings(c.notch);
            compare(left.length, 0, name + " still running: " + left.join(", "));
            compare(c.face.frozen, true);
        }
    }

    function test_freeze_holds_every_frame_and_pointer_is_ignored() {
        const c = lively("isaac");
        const art = c.face.art;
        tryVerify(function() { return art.flowFrame > 0 && c.notch.frame > 0; }, 1500);
        // A blink and a tear in flight when the game starts are closed out, not frozen mid-way.
        findChild(c.face, "companionBlink").start();
        art.shoot();
        c.notch.companionFrozen = true;
        compare(c.face.blink, 1);
        verify(!findChild(c.face, "isaacShot").visible, "no tear hanging in mid-air");
        const frame = c.notch.frame, flow = art.flowFrame, pulse = c.notch.workPulse, x = c.face.shownX;
        let changes = 0;
        const seen = [];
        const watch = function(name) { return function() { changes++; seen.push(name); }; };
        const bump = [watch("frame"), watch("flow"), watch("pulse"), watch("blink"), watch("x"), watch("y"), watch("lid")];
        c.notch.frameChanged.connect(bump[0]); art.flowFrameChanged.connect(bump[1]);
        c.notch.workPulseChanged.connect(bump[2]); c.face.blinkChanged.connect(bump[3]);
        c.face.shownXChanged.connect(bump[4]); c.face.shownYChanged.connect(bump[5]); c.face.lidChanged.connect(bump[6]);
        // The pointer keeps moving (the relay is not even listened to by main.qml, but the
        // face must not react if a stale event slips through).
        c.notch.pointerX = c.centre.x - 900; c.notch.pointerAt = Date.now() + 5;
        wait(700);
        compare(changes, 0, "a frozen notch redraws nothing: " + seen.join());
        compare(c.notch.frame, frame); compare(art.flowFrame, flow);
        compare(c.notch.workPulse, pulse); compare(c.face.shownX, x);
    }

    function test_freeze_hides_and_resets_nothing_and_thaw_resumes() {
        const c = lively("isaac");
        c.notch.waiting = true;
        const stepper = findChild(c.notch, "notchStepper");
        tryVerify(function() { return stepper.running && c.notch.frame > 2; }, 1500);
        c.notch.companionFrozen = true;
        const frame = c.notch.frame;
        compare(c.face.visible, true); compare(c.face.opacity, 1);
        compare(c.face.waiting, true, "state kept");
        compare(c.face.mood, "waiting");
        compare(c.notch.pulsing, true);
        compare(findChild(c.notch, "panelClock").text, "11:59 PM", "the clock itself is untouched");
        // Music bars hold their frame instead of collapsing to rest.
        const bars = [0, 1, 2, 3].map(function(i) { return findChild(c.notch, "notchMusicBar" + i).height; });
        wait(300);
        compare(c.notch.frame, frame);
        compare([0, 1, 2, 3].map(function(i) { return findChild(c.notch, "notchMusicBar" + i).height; }).join(), bars.join());
        verify(bars.some(function(h) { return h > 2; }), "bars still show the playing state");
        // Thaw: the same clock carries on from where it stopped (no reset to frame 0).
        c.notch.companionFrozen = false;
        tryVerify(function() { return stepper.running && c.notch.frame !== frame; }, 1500);
        compare(c.face.waiting, true);
        // The pointer is followed again (waiting ignores it, so look at an ordinary face).
        c.notch.waiting = false;
        const before = c.face.shownX;
        c.notch.pointerX = c.centre.x - 2000; c.notch.pointerAt = Date.now() + 10;
        tryVerify(function() { return c.face.shownX < before; }, 1000);
    }

    function test_blinks_and_one_clock_cost_few_redraws() {
        const c = lively("isaac");
        c.notch.playing = true;
        const art = c.face.art;
        // One clock: with music and Isaac's tears both on, the flow steps on the pill's ticks,
        // not on a second timer.
        tryVerify(function() { return c.notch.frame > 0; }, 1500);
        let ticks = 0, flow = 0;
        const t = function() { ticks++; }, f = function() { flow++; };
        c.notch.frameChanged.connect(t); art.flowFrameChanged.connect(f);
        wait(1500);
        c.notch.frameChanged.disconnect(t); art.flowFrameChanged.disconnect(f);
        verify(ticks >= 9 && ticks <= 14, "clock ticks " + ticks);
        verify(Math.abs(ticks - flow) <= 1, "flow " + flow + " vs clock " + ticks);
        // A blink is three held frames, not a frame-rate tween.
        c.notch.playing = false; c.notch.working = false;
        c.notch.companionFrozen = true;      // quiet everything else, then blink once by hand
        c.notch.companionFrozen = false;
        let changes = 0;
        const b = function() { changes++; };
        c.face.blinkChanged.connect(b);
        const blink = findChild(c.face, "companionBlink");
        blink.start();
        tryVerify(function() { return !blink.running; }, 1000);
        c.face.blinkChanged.disconnect(b);
        compare(c.face.blink, 1);
        verify(changes >= 2 && changes <= 4, "a blink redraws " + changes + " times");
    }

    function test_clock_is_off_when_nothing_needs_it() {
        const c = lively("isaac");
        c.notch.playing = false; c.notch.working = false;
        const stepper = findChild(c.notch, "notchStepper");
        tryVerify(function() { return stepper.running; }, 1000);          // tears flowing
        c.face.settled = true;
        tryVerify(function() { return !stepper.running; }, 1000);         // settled: nothing ticks
        c.face.settled = false;
        c.notch.companion = false;
        tryVerify(function() { return !stepper.running; }, 1000);         // companion off
    }
}
