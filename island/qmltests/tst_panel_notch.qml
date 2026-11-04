import QtQuick
import QtTest
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
