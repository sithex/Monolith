import QtQuick
import QtQuick.Window

// The notch companion's brain: where it looks, when it blinks, dozes, ponders and smiles.
// What it looks like is a character drawn by a file in characters/ (see `characters`);
// each one gets this item as `face` and draws itself from its state.
//
// Full guide (performance, fidelity, tests, verifying): docs/notch-companion-characters.md
//
// Adding a character: draw it in characters/<Name>.qml (a `property Item face`, plus
// optional `preferredSize` and `gazeScale`), list it in `characters` below, add the
// variant to `CompanionCharacter` in the island's settings contract and a tile to
// ui/island/companionCharacters.tsx.
//
// The global pointer arrives as a D-Bus signal (see main.qml), already throttled at the
// source and sent only while the pointer moves; everything here is event-driven except
// a blink timer that runs only while the face is shown.
//
// Redraws are the cost (each one repaints the panel), so the gaze is kept in quarter
// pixels: a pointer update that would not visibly move an eye changes nothing and draws
// nothing. Small steps land in one frame; only real jumps (a flick, a glance) glide.
Item {
    id: face
    // Character id from the island's settings; unknown ids fall back to the blob.
    property string character: "blob"
    readonly property var characters: ({ blob: "characters/Blob.qml", isaac: "characters/Isaac.qml", notchmimic: "characters/NotchMimic.qml", companioncube: "characters/CompanionCube.qml", nurgling: "characters/Nurgling.qml", hollowknight: "characters/HollowKnight.qml", frog: "characters/Frog.qml", necronwarrior: "characters/NecronWarrior.qml", necronoverlord: "characters/NecronOverlord.qml", canoptekscarab: "characters/CanoptekScarab.qml", eyeofsauron: "characters/EyeOfSauron.qml", mushroom: "characters/Mushroom.qml", sonichu: "characters/Sonichu.qml" })
    readonly property string shownCharacter: Object.prototype.hasOwnProperty.call(characters, character) ? character : "blob"
    readonly property Item art: artLoader.item
    // The square the current character wants in the notch.
    readonly property real preferredSize: art && art.preferredSize > 0 ? art.preferredSize : 16

    // Global pointer position in logical px and when it last moved (ms, 0 = never).
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    property bool happy: false      // pointer over the notch
    property bool working: false    // Hermes is busy: a thinking glance when the pointer rests
    property bool waiting: false    // Hermes waits for you: wide eyes on you, quick blinks, "!"
    // Settings → companionReactive. Off: no Hermes moods and no dozing, just gaze and blinks.
    property bool reactive: true
    // What a character may draw from, besides the details below: idle | working | waiting |
    // sleeping. Waiting wins over working; both wake it.
    readonly property string mood: !reactive ? "idle" : waiting ? "waiting" : working ? "working" : sleepy ? "sleeping" : "idle"
    property color alertColor: "#f5a623"
    property bool motion: true      // false when the desktop disables animations
    // Game mode (Settings → gameModeFreezeCompanion): every animation and timer of the face
    // stops and its state is kept; nothing is reset or hidden, and it picks up on thaw.
    property bool frozen: false
    // The notch's one stepped clock (PanelPill.frame, ~8 ticks a second). A character that
    // wants steady motion (Isaac's tear flow) advances on it instead of running a timer of
    // its own, so the clock's repaint and the character's are the same repaint.
    property int frame: 0
    readonly property bool clockWanted: !!art && art.clockWanted === true
    property color bodyColor: "#f1ede4"
    property color eyeColor: "#171717"
    readonly property real size: Math.min(width, height)

    // Eye offset toward the pointer in shares of the face size, inside the face.
    property real lookX: 0
    property real lookY: 0
    // The offset actually drawn, in logical px on a quarter-pixel grid.
    property real shownX: 0
    property real shownY: 0
    property bool glide: false
    Behavior on shownX { enabled: face.motion && face.glide && !face.frozen; NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
    Behavior on shownY { enabled: face.motion && face.glide && !face.frozen; NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
    property real blink: 1          // vertical eye scale; dips to ~0 for a blink
    property bool sleepy: false
    // The pointer hasn't moved for a minute: constant animation (Isaac's flow) rests and
    // blinks come rarer, long before the ten-minute doze. Reading or typing with the mouse
    // parked is the common state, so this is where steady redraws would cost the most.
    property bool settled: false
    // How open the eyes are when not blinking: heavy-lidded while dozing.
    property real lid: sleepy ? .42 : waiting && reactive ? 1.15 : 1
    // Eases only where the pointer is; other notches change once, in a single frame.
    Behavior on lid { enabled: face.motion && face.attended && !face.frozen; NumberAnimation { duration: 600; easing.type: Easing.InOutSine } }
    property bool thinking: false
    property real glance: 0         // a side look now and then while nothing moves
    // After each idle blink: for characters with a habit of their own (tears, a twitch).
    // `resting` = the pointer has been still for a few seconds.
    signal blinked(bool resting)

    // True on the monitor the pointer is on. With a notch on every monitor only that
    // notch's companion moves, so one panel redraws instead of all of them; until the
    // pointer first moves, none do.
    // This notch's monitor in global logical px.
    property rect screenArea: Qt.rect(Screen.virtualX, Screen.virtualY, Screen.width, Screen.height)
    readonly property bool attended: pointerAt > 0 && pointerHere(pointerX, pointerY)
    function pointerHere(x, y) {
        return x >= screenArea.x && x < screenArea.x + screenArea.width
            && y >= screenArea.y && y < screenArea.y + screenArea.height;
    }

    readonly property real maxX: .13
    readonly property real maxY: .09

    function updateLook() {
        if (pointerAt <= 0) return;
        const centre = face.mapToGlobal(face.width / 2, face.height / 2);
        const dx = pointerX - centre.x, dy = pointerY - centre.y;
        // Soft saturation: near the notch the eyes track closely, across the screen they
        // settle at the edge of the face instead of snapping there.
        lookX = maxX * dx / (Math.abs(dx) + 160);
        lookY = maxY * dy / (Math.abs(dy) + 90);
        retarget();
    }
    function quarter(v) { return Math.round(v * 4) / 4; }
    // Where the eyes point: the pointer, a thinking look, a doze or a passing glance.
    function retarget() {
        const reach = art && art.gazeScale > 0 ? art.gazeScale : 1;
        // Waiting looks straight out at you and ignores the pointer, so it redraws nothing.
        const sx = waiting ? 0 : thinking ? .08 : sleepy ? 0 : lookX + glance;
        const sy = waiting ? .04 : thinking ? -.07 : sleepy ? .05 : lookY;
        const tx = quarter(sx * size * reach), ty = quarter(sy * size * reach);
        // Compare with where the eyes are headed, not where a glide has got to so far:
        // a target reversed mid-glide would otherwise look unchanged, and the glide
        // would finish at the stale target and stay there.
        if (tx === targetX && ty === targetY) return;
        glide = Math.abs(tx - shownX) >= .75 || Math.abs(ty - shownY) >= .75;
        targetX = tx;
        targetY = ty;
        shownX = tx;
        shownY = ty;
    }
    property real targetX: 0
    property real targetY: 0
    onThinkingChanged: retarget()
    onWaitingChanged: { if (waiting) sleepy = false; retarget(); }
    onSleepyChanged: retarget()
    onGlanceChanged: retarget()
    onSizeChanged: retarget()
    onArtChanged: retarget()
    // On another monitor's notch the whole face holds still (no gaze, blinks, glances,
    // dozing or thinking looks), so with a notch on every monitor only the one you're
    // on ever redraws. It picks up again the moment the pointer arrives.
    onPointerAtChanged: {
        if (frozen) return;
        // Any move anywhere is activity: every notch's doze clock restarts and a dozing
        // face wakes (only the one under the pointer animates the wake-up).
        const here = pointerAt > 0 && pointerHere(pointerX, pointerY);
        if (sleepy && here && motion) wake.restart();
        sleepy = false;
        settled = false;
        if (drowsy.running) drowsy.restart();
        if (!here) { ponder.stop(); settle.stop(); return; }
        settle.restart();
        updateLook();
        thinking = false;
        if (working) ponder.restart();
    }
    onWorkingChanged: {
        if (working) { sleepy = false; if (attended && !frozen) ponder.restart(); }
        else { ponder.stop(); if (attended && !frozen) thinking = false; }
    }
    onVisibleChanged: if (visible && attended && !frozen) updateLook()
    // Freeze: stop every timer and animation and leave the face as it is (a blink in progress
    // is closed out to open eyes, a side glance ends). Thaw: pick the pointer up again; the
    // doze and settle clocks start over, nothing else resets.
    onFrozenChanged: {
        if (frozen) {
            wake.stop(); blinkOnce.stop(); blink = 1;
            unglance.stop(); glance = 0;
            ponder.stop(); settle.stop(); blinker.stop();
        } else if (attended) {
            settle.restart();
            updateLook();
            if (working && !thinking) ponder.restart();
            if (visible && motion && !sleepy && !happy) blinker.restart();
        }
    }

    // Dozes off after ten minutes with no pointer move and no Hermes turn, on every notch
    // at once (one still frame each); wakes with a wide-eyed blink. A one-shot, not a loop.
    Timer { id: drowsy; objectName: "companionDrowsy"; interval: 600000; running: face.visible && !face.frozen && face.reactive && !face.working && !face.waiting; onTriggered: face.sleepy = true }
    Timer { id: settle; objectName: "companionSettle"; interval: 60000; onTriggered: face.settled = true }
    Timer { id: ponder; interval: 1500; onTriggered: face.thinking = face.working && face.attended }
    // Blinks are stepped, like the pixel art they animate: a tween at frame rate repainted the
    // panel ~15 times per blink on every blink; three held frames repaint it three times.
    SequentialAnimation {
        id: wake
        PropertyAction { target: face; property: "blink"; value: 1.25 }
        PauseAnimation { duration: 130 }
        PropertyAction { target: face; property: "blink"; value: 1.1 }
        PauseAnimation { duration: 110 }
        PropertyAction { target: face; property: "blink"; value: 1 }
    }
    SequentialAnimation {
        id: blinkOnce
        objectName: "companionBlink"
        PropertyAction { target: face; property: "blink"; value: .08 }
        PauseAnimation { duration: 100 }
        PropertyAction { target: face; property: "blink"; value: .5 }
        PauseAnimation { duration: 60 }
        PropertyAction { target: face; property: "blink"; value: 1 }
    }
    Timer {
        id: blinker
        objectName: "companionBlinker"
        interval: 3000
        running: face.visible && !face.frozen && face.attended && face.motion && !face.sleepy && !face.happy
        onTriggered: {
            blinkOnce.restart();
            const resting = Date.now() - face.pointerAt > 4000;
            // Now and then, with the pointer resting, the eyes wander to one side and back.
            if (resting && !face.thinking && Math.random() < .25) {
                face.glance = (Math.random() < .5 ? -1 : 1) * .09;
                unglance.restart();
            }
            face.blinked(resting);
            // Waiting for an answer: quicker, expectant blinks (same timer, shorter gaps).
            interval = face.mood === "waiting" ? 1600 + Math.random() * 1000
                : face.settled ? 9000 + Math.random() * 6000 : 2600 + Math.random() * 3800;
            restart();
        }
    }
    Timer { id: unglance; interval: 1300; onTriggered: face.glance = 0 }

    // The mood as one static glyph at the face's shoulder, the same for every character:
    // "!" while Hermes waits, "z" while dozing. Drawn once per change, never animated.
    Text {
        id: moodMark
        objectName: "moodMark"
        z: 1
        x: face.width - 3
        y: -3
        visible: face.mood === "waiting" || face.mood === "sleeping"
        text: face.mood === "waiting" ? "!" : "z"
        color: face.mood === "waiting" ? face.alertColor : face.bodyColor
        opacity: face.mood === "waiting" ? 1 : .7
        font.pixelSize: 8
        font.weight: Font.Black
    }
    Loader {
        id: artLoader
        anchors.fill: parent
        function load() { setSource(Qt.resolvedUrl(face.characters[face.shownCharacter]), { face: face }); }
        Component.onCompleted: load()
        Connections { target: face; function onShownCharacterChanged() { artLoader.load(); } }
    }
}
