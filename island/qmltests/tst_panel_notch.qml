import QtQuick
import QtTest
import QtQuick.Window
import "../plasma-anchor/contents/ui" as Panel

TestCase {
    id: testCase
    name: "PanelNotch"
    width: 400
    height: 140
    visible: true
    when: windowShown
    Component {
        id: factory
        Panel.PanelPill { x: 100; y: 50; width: 144; height: 26; clockText: "11:59 PM"; dateText: "Sun, Nov 1" }
    }
    SignalSpy { id: clickSpy; signalName: "activated" }
    function test_border_controls_respect_disabled_zero_and_thickness() {
        const notch = createTemporaryObject(factory, testCase);
        compare(notch.borderEnabled, true);
        notch.borderWidth = 4; notch.borderOpacity = .6; notch.borderColor = "#abcdef";
        const outline = findChild(notch, "notchOutline");
        compare(outline.strokeWidth, 4); fuzzyCompare(outline.strokeColor.a, .6, .01);
        notch.borderOpacity = 0; fuzzyCompare(outline.strokeColor.a, 0, .001);
        notch.borderEnabled = false; compare(outline.strokeWidth, 0);
    }
    function test_companion_eyes_follow_the_pointer_and_stay_clear() {
        const notch = createTemporaryObject(factory, testCase);
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const eye = findChild(notch, "companionEye0");
        const clock = findChild(notch, "panelClock");
        // Off by default in the bare pill; main.qml turns it on from the island's settings.
        verify(!face.visible);
        notch.companion = true;
        tryCompare(face, "opacity", 1);
        verify(face.x + face.width < clock.mapToItem(notch, 0, 0).x);
        const restX = eye.x, restY = eye.y;
        // Pointer far to the right and below: the eyes go right and down, within the face.
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x + 2000; notch.pointerY = centre.y + 900; notch.pointerAt = Date.now();
        tryVerify(function() { return eye.x > restX + 1 && eye.y > restY; }, 2000, eye.x + "," + eye.y);
        verify(eye.x + eye.width <= face.width);
        // Far to the left and above: the other way.
        notch.pointerX = centre.x - 2000; notch.pointerY = centre.y - 900; notch.pointerAt = Date.now() + 1;
        tryVerify(function() { return eye.x < restX - 1 && eye.y < restY; }, 2000, eye.x + "," + eye.y);
        verify(eye.x >= 0);
        // The mic mark moves past the face and both stay clear of the clock.
        const mic = findChild(notch, "notchMicMuted");
        notch.micMuted = true;
        tryCompare(mic, "opacity", .85);
        verify(face.visible);
        verify(face.x + face.width <= mic.x);
        verify(mic.x + mic.width < clock.mapToItem(notch, 0, 0).x);
        // A narrow notch gives the mic mark the companion's room.
        notch.width = 120;
        tryCompare(face, "opacity", 0);
        notch.companion = false; notch.micMuted = false; notch.width = 144;
        tryCompare(face, "opacity", 0);
    }
    function test_companion_ignores_moves_it_could_not_show() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x + 2500; notch.pointerY = centre.y + 900; notch.pointerAt = Date.now();
        tryVerify(function() { return face.shownX > 1.5; }, 1000);
        wait(200);
        const x = face.shownX, y = face.shownY;
        // On the grid, so nothing in between gets drawn.
        compare(x * 4, Math.round(x * 4)); compare(y * 4, Math.round(y * 4));
        // Far away, a 40 px move cannot visibly turn an eye: nothing changes, nothing animates.
        let changes = 0;
        const count = function() { changes++; };
        face.shownXChanged.connect(count); face.shownYChanged.connect(count);
        notch.pointerX = centre.x + 2540; notch.pointerAt = Date.now() + 1;
        notch.pointerY = centre.y + 930; notch.pointerAt = Date.now() + 2;
        compare(changes, 0);
        // A small step near the notch lands at once, without a glide.
        notch.pointerX = centre.x + 40; notch.pointerY = centre.y; notch.pointerAt = Date.now() + 3;
        tryVerify(function() { return face.shownX < 1.25; }, 1000);
        wait(200); // let that long glide land first
        const before = face.shownX;
        notch.pointerX = centre.x + 46; notch.pointerAt = Date.now() + 4;
        // Lands at once: no glide left in flight toward the target.
        compare(face.shownX, face.targetX);
        verify(face.shownX - before <= .5);
        face.shownXChanged.disconnect(count); face.shownYChanged.disconnect(count);
    }
    function test_companion_characters_switch_and_isaac_cries() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const clock = findChild(notch, "panelClock");
        compare(face.shownCharacter, "blob");
        compare(face.width, 16);
        notch.companionCharacter = "isaac";
        compare(face.shownCharacter, "isaac");
        compare(face.width, 18);
        verify(findChild(face, "isaacMouth"));
        // His eyes follow the pointer too, and stay inside his head.
        const eye = findChild(face, "companionEye0");
        const restX = eye.x;
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x + 2500; notch.pointerY = centre.y; notch.pointerAt = Date.now();
        tryVerify(function() { return eye.x > restX + 1; }, 1000);
        verify(eye.x + eye.width <= face.width);
        // Always crying; the tears dry while the pointer is on the notch.
        const stream = findChild(face, "isaacStream");
        verify(stream.visible);
        compare(findChild(face, "isaacMouth").opacity, 1);
        face.happy = true;
        tryVerify(function() { return stream.opacity < .2; }, 1000);
        tryVerify(function() { return findChild(face, "isaacSmile").opacity > .9; }, 1000);
        face.happy = false;
        tryVerify(function() { return stream.opacity > .9; }, 1000);
        // Choppy flow: the bands step at 8 ticks a second, no faster, and stop on hover.
        notch.pointerX = centre.x; notch.pointerY = centre.y + 40; notch.pointerAt = Date.now() + 5;
        verify(face.attended);
        const art = face.art;
        let ticks = 0;
        const tick = function() { ticks++; };
        art.flowFrameChanged.connect(tick);
        wait(1000);
        art.flowFrameChanged.disconnect(tick);
        verify(ticks >= 6 && ticks <= 9, "flow ticked " + ticks + " times in a second");
        verify(findChild(face, "isaacFlow"));
        verify(!findChild(face, "isaacDrip").visible);
        face.happy = true;
        const frame = art.flowFrame;
        wait(400);
        compare(art.flowFrame, frame);
        face.happy = false;
        // Pointer on another monitor: this notch's flow rests (one panel redraws, not all).
        const eyeBefore = findChild(face, "companionEye0").x;
        notch.pointerX = 150000; notch.pointerAt = Date.now() + 10;
        verify(!face.attended);
        const still = art.flowFrame;
        // The whole face holds still: no gaze, no blink, no flow.
        let changed = 0;
        const any = function() { changed++; };
        face.shownXChanged.connect(any); face.shownYChanged.connect(any); face.blinkChanged.connect(any);
        notch.pointerX = 150900; notch.pointerY = -300; notch.pointerAt = Date.now() + 12;
        wait(400);
        compare(art.flowFrame, still);
        compare(changed, 0);
        compare(findChild(face, "companionEye0").x, eyeBefore);
        verify(!findChild(face, "isaacShot").visible);
        face.shownXChanged.disconnect(any); face.shownYChanged.disconnect(any); face.blinkChanged.disconnect(any);
        notch.pointerX = centre.x + 10; notch.pointerY = centre.y + 10; notch.pointerAt = Date.now() + 11;
        verify(face.attended);
        tryVerify(function() { return art.flowFrame !== still; }, 1000);
        // Idle shot: a tear flies, splashes and is gone, nothing left running.
        const shot = findChild(face, "isaacShot");
        verify(!shot.visible);
        face.art.shoot();
        verify(shot.visible);
        tryVerify(function() { return findChild(face, "isaacSplash").visible; }, 1000);
        tryVerify(function() { return !shot.visible && !findChild(face, "isaacSplash").visible; }, 2000);
        // The mute mark moves past the bigger head and stays clear of the clock.
        const mic = findChild(notch, "notchMicMuted");
        notch.micMuted = true;
        tryCompare(mic, "opacity", .85);
        verify(face.x + face.width <= mic.x);
        verify(mic.x + mic.width < clock.mapToItem(notch, 0, 0).x);
        // An unknown character (older or newer settings) falls back to the blob.
        notch.companionCharacter = "ghost";
        compare(face.shownCharacter, "blob");
        compare(face.width, 16);
        verify(findChild(face, "companionBody"));
    }
    function test_main_component_compiles() {
        const component = Qt.createComponent("../plasma-anchor/contents/ui/main.qml");
        compare(component.status, Component.Ready, component.errorString());
    }
    function test_live_states_timer_working_and_mic() {
        const notch = createTemporaryObject(factory, testCase);
        const date = findChild(notch, "panelDate");
        const outline = findChild(notch, "notchOutline");
        const mic = findChild(notch, "notchMicMuted");
        compare(date.text, "Sun, Nov 1");
        // Running countdown replaces the date line and ticks down in mm:ss.
        notch.timerEndsAt = Date.now() + 65500;
        tryVerify(function() { return /^1:0[56]$/.test(date.text); }, 2000, date.text);
        compare(date.color, notch.accent);
        // Paused: static remaining time with a pause mark.
        notch.timerEndsAt = 0; notch.timerPausedMs = 3723000;
        compare(date.text, "❚❚ 1:02:03");
        notch.timerPausedMs = -1;
        compare(date.text, "Sun, Nov 1");
        // Timer still shows with the date hidden.
        notch.showDate = false; notch.timerEndsAt = Date.now() + 9000;
        verify(date.visible);
        notch.timerEndsAt = 0;
        verify(!date.visible);
        // Hermes working: accent breath on the border, gone when it stops.
        const idleStroke = outline.strokeColor;
        notch.working = true;
        tryVerify(function() { return notch.workPulse > .3; }, 2500);
        verify(outline.strokeWidth >= 1);
        notch.working = false;
        compare(outline.strokeColor, idleStroke);
        // Mic muted mark stays clear of the clock and the music bars.
        notch.micMuted = true;
        tryCompare(mic, "opacity", .85);
        verify(mic.x + mic.width < findChild(notch, "panelClock").mapToItem(notch, 0, 0).x);
        notch.micMuted = false;
        tryCompare(mic, "opacity", 0);
        compare(notch.width, 144); compare(notch.height, 26);
    }
    function test_waiting_pulse_is_amber_slower_and_wins_over_working() {
        const notch = createTemporaryObject(factory, testCase);
        const outline = findChild(notch, "notchOutline");
        const stepper = findChild(notch, "notchStepper");
        const idleStroke = outline.strokeColor;
        notch.working = true;
        compare(notch.pulseColor, notch.accent);
        notch.waiting = true;
        compare(notch.pulseColor, notch.alertColor, "waiting takes precedence");
        verify(stepper.running, "same stepped clock, no new timer");
        verify(/waiting for you/.test(notch.Accessible.description));
        tryVerify(function() { return notch.workPulse > .3; }, 3000);
        // Amber stroke: red > green > blue, whatever the accent.
        const c = outline.strokeColor;
        verify(c.r > c.g && c.g > c.b, String(c));
        // Slower breath: half the working rate, one step per two ticks (odd ticks draw nothing).
        notch.frame = 18;
        compare(notch.workPulse, 1);
        let changes = 0;
        const count = function() { changes++; };
        notch.workPulseChanged.connect(count);
        notch.frame = 19;
        notch.workPulseChanged.disconnect(count);
        compare(changes, 0);
        notch.frame = 9;
        notch.waiting = false;
        compare(notch.workPulse, 1, "working peaks at step 9");
        // Pulse switched off in settings: waiting falls back to plain working.
        notch.waitingPulse = false; notch.waiting = true;
        compare(notch.pulseColor, notch.accent);
        notch.working = false;
        compare(notch.pulsing, false);
        compare(outline.strokeColor, idleStroke);
        compare(stepper.running, false);
    }
    function test_companion_moods_react_to_hermes_and_doze() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const mark = findChild(face, "moodMark");
        const drowsy = findChild(face, "companionDrowsy");
        tryCompare(face, "opacity", 1);
        compare(face.mood, "idle");
        verify(!mark.visible);
        compare(drowsy.interval, 600000, "dozes after ten minutes");
        verify(drowsy.running);
        notch.working = true;
        compare(face.mood, "working");
        verify(!drowsy.running, "never dozes during a turn");
        notch.waiting = true;
        compare(face.mood, "waiting", "waiting wins over working");
        verify(mark.visible); compare(mark.text, "!");
        // Waiting looks straight at you: pointer moves don't redraw the eyes.
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x + 3000; notch.pointerAt = Date.now();
        wait(250);
        let moved = 0;
        const count = function() { moved++; };
        face.shownXChanged.connect(count); face.shownYChanged.connect(count);
        notch.pointerX = centre.x - 3000; notch.pointerAt = Date.now() + 1;
        wait(200);
        face.shownXChanged.disconnect(count); face.shownYChanged.disconnect(count);
        compare(moved, 0);
        notch.waiting = false; notch.working = false;
        verify(drowsy.running);
        face.sleepy = true;
        compare(face.mood, "sleeping");
        compare(mark.text, "z");
        // Any pointer move wakes every notch, even one the pointer isn't on.
        notch.pointerX = 150000; notch.pointerAt = Date.now() + 2;
        verify(!face.attended);
        compare(face.sleepy, false);
        // Hermes asking also wakes it.
        face.sleepy = true; notch.waiting = true;
        compare(face.sleepy, false);
        notch.waiting = false;
        // Reactive off: no moods, no doze, just the gaze.
        notch.companionReactive = false;
        notch.working = true; notch.waiting = true;
        compare(face.mood, "idle");
        verify(!mark.visible);
        verify(!drowsy.running);
    }
    function test_every_character_shows_the_moods_without_new_timers() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        tryCompare(face, "opacity", 1);
        for (const name of ["blob", "isaac", "notchmimic", "companioncube", "nurgling", "hollowknight", "frog", "necronwarrior", "necronoverlord", "canoptekscarab", "eyeofsauron", "salamander"]) {
            notch.companionCharacter = name;
            compare(face.shownCharacter, name);
            notch.waiting = true;
            verify(findChild(face, "moodMark").visible, name);
            if (face.art.habitRunning !== undefined)
                compare(face.art.allowed, false, name + " holds still while Hermes waits");
            if (name === "companioncube")
                compare(face.art.heartColors.H, String(notch.alertColor));
            notch.waiting = false;
            face.sleepy = true;
            compare(findChild(face, "moodMark").text, "z", name);
            face.sleepy = false;
        }
    }
    function test_settled_pointer_rests_the_constant_flow() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        notch.companionCharacter = "isaac";
        const face = findChild(notch, "notchCompanion");
        face.screenArea = Qt.rect(-100000, -100000, 200000, 200000);
        const settle = findChild(face, "companionSettle");
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        notch.pointerX = centre.x; notch.pointerY = centre.y + 40; notch.pointerAt = Date.now();
        verify(settle.running); compare(settle.interval, 60000);
        const art = face.art;
        tryVerify(function() { return art.flowFrame > 0; }, 1000);
        face.settled = true;
        const frame = art.flowFrame;
        wait(400);
        compare(art.flowFrame, frame, "no ticks with the pointer parked");
        notch.pointerY = centre.y + 44; notch.pointerAt = Date.now() + 1;
        compare(face.settled, false);
        tryVerify(function() { return art.flowFrame !== frame; }, 1000);
    }
    function test_hover_peek_shows_track_or_timer_after_the_hover_delay() {
        const notch = createTemporaryObject(factory, testCase);
        const peek = findChild(notch, "notchPeek");
        const clock = findChild(notch, "panelClock");
        notch.hoverDelay = 250;
        notch.playing = true;
        notch.peekTrack = "A Very Long Song Title That Cannot Fit — Some Artist";
        mouseMove(notch, 72, 13);
        tryCompare(notch, "hovered", true);
        compare(notch.peeking, false, "waits for the hover delay");
        tryCompare(notch, "peeking", true, 1000);
        tryCompare(peek, "opacity", 1);
        compare(peek.text, notch.peekTrack);
        compare(peek.elide, Text.ElideRight);
        verify(peek.truncated, "ellipsized, never a marquee");
        verify(peek.x >= 0 && peek.x + peek.width <= findChild(notch, "notchMusicIndicator").x);
        tryVerify(function() { return clock.parent.opacity === 0; }, 1000);
        mouseMove(testCase, 5, 5);
        tryCompare(notch, "peeking", false);
        tryCompare(peek, "opacity", 0);
        // A timer when nothing plays.
        notch.playing = false;
        notch.timerEndsAt = Date.now() + 65500;
        mouseMove(notch, 72, 13);
        tryCompare(notch, "peeking", true, 1000);
        verify(/^Timer 1:0[56] left$/.test(peek.text), peek.text);
        mouseMove(testCase, 5, 5);
        // Nothing to say: no peek, the clock stays.
        notch.timerEndsAt = 0;
        mouseMove(notch, 72, 13);
        wait(400);
        compare(notch.peeking, false);
        compare(clock.parent.opacity, 1);
        mouseMove(testCase, 5, 5);
        // Setting off.
        notch.hoverPeek = false; notch.playing = true;
        mouseMove(notch, 72, 13);
        wait(400);
        compare(notch.peeking, false);
        mouseMove(testCase, 5, 5);
    }
    function test_companion_shows_only_on_the_chosen_monitor() {
        const notch = createTemporaryObject(factory, testCase);
        notch.companion = true;
        notch.screenName = "DP-1";
        const face = findChild(notch, "notchCompanion");
        tryCompare(face, "opacity", 1);
        notch.companionMonitor = "DP-2";
        compare(notch.companionShown, false);
        tryCompare(face, "opacity", 0);
        verify(!face.visible);
        notch.companionMonitor = "DP-1";
        tryCompare(face, "opacity", 1);
        notch.companionMonitor = "";
        compare(notch.companionShown, true);
    }
    function test_motion_is_stepped_and_only_where_the_pointer_is() {
        const notch = createTemporaryObject(factory, testCase);
        const stepper = findChild(notch, "notchStepper");
        compare(stepper.running, false, "nothing ticks while idle");
        notch.playing = true;
        verify(stepper.running);
        verify(stepper.interval >= 100, "a stepped clock, not frame rate");
        const start = notch.frame;
        tryVerify(function() { return notch.frame > start + 2; }, 1500);
        for (let i = 0; i < 4; ++i)
            verify(Number.isInteger(findChild(notch, "notchMusicBar" + i).height), "bars move in whole pixels");
        // Companion on and the pointer on another monitor: hold a still frame here.
        notch.companion = true;
        notch.pointerX = -50000; notch.pointerY = -50000; notch.pointerAt = Date.now();
        compare(stepper.running, false);
        notch.working = true;
        compare(notch.workPulse, .6, "a still, visible working tint");
        notch.working = false; notch.playing = false;
        compare(notch.workPulse, 0);
    }
    function test_music_indicator_only_animates_while_playing_without_resizing() {
        const notch = createTemporaryObject(factory, testCase);
        const indicator = findChild(notch, "notchMusicIndicator");
        compare(indicator.opacity, 0); compare(indicator.animating, false);
        notch.playing = true;
        tryCompare(indicator, "opacity", 1);
        verify(indicator.animating);
        compare(notch.width, 144); compare(notch.height, 26);
        verify(indicator.x >= 0 && indicator.x + indicator.width <= notch.width);
        for (const name of ["panelClock", "panelDate"]) {
            const text = findChild(notch, name);
            verify(text.mapToItem(notch, 0, 0).x + text.width < indicator.x, "clock/date must not overlap bars");
        }
        notch.playing = false;
        compare(indicator.animating, false);
        tryCompare(indicator, "opacity", 0);
        compare(findChild(notch, "notchMusicBar0").height, 2);
    }
    function test_hover_animates_inside_fixed_panel_bounds() {
        const notch = createTemporaryObject(factory, testCase);
        verify(notch !== null);
        mouseMove(testCase, 5, 5);
        wait(300);
        compare(notch.hoverProgress, 0);
        const surface = findChild(notch, "notchSurface");
        verify(surface !== null, "the native panel must draw a notch, not a rounded Rectangle");
        const oldWidth = surface.width;
        mouseMove(notch, 72, 13);
        tryCompare(notch, "hovered", true);
        tryVerify(function() { return notch.hoverProgress > .99; });
        compare(surface.width, oldWidth); // Static outline is shared with the withdrawal endpoint.
        compare(notch.width, 144); compare(notch.height, 26);
        verify(surface.x >= 0 && surface.y >= 0);
        verify(surface.x + surface.width <= notch.width + .01);
        verify(surface.y + surface.height <= notch.height + .01);
        mouseMove(testCase, 5, 5);
        tryCompare(notch, "hovered", false);
        tryCompare(notch, "hoverProgress", 0);
    }
    function test_clock_and_date_fit_the_26px_taskbar_slot() {
        const notch = createTemporaryObject(factory, testCase);
        for (const name of ["panelClock", "panelDate"]) {
            const text = findChild(notch, name);
            const pos = text.mapToItem(notch, 0, 0);
            verify(pos.x >= 0 && pos.y >= 0, name + " begins inside the notch");
            verify(pos.x + text.width <= notch.width && pos.y + text.height <= notch.height, name + " ends inside the notch");
        }
    }
    function test_click_and_keyboard_still_activate_once() {
        const notch = createTemporaryObject(factory, testCase);
        clickSpy.target = notch; clickSpy.clear();
        mouseClick(notch, 72, 13);
        compare(clickSpy.count, 1);
        notch.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(clickSpy.count, 2);
        clickSpy.target = null;
    }
}
