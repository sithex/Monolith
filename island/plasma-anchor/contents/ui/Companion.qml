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
    readonly property var characters: ({ blob: "characters/Blob.qml", isaac: "characters/Isaac.qml" })
    readonly property string shownCharacter: characters[character] ? character : "blob"
    readonly property Item art: artLoader.item
    // The square the current character wants in the notch.
    readonly property real preferredSize: art && art.preferredSize > 0 ? art.preferredSize : 16

    // Global pointer position in logical px and when it last moved (ms, 0 = never).
    property real pointerX: 0
    property real pointerY: 0
    property real pointerAt: 0
    property bool happy: false      // pointer over the notch
    property bool working: false    // Hermes is busy: a thinking glance when the pointer rests
    property bool motion: true      // false when the desktop disables animations
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
    Behavior on shownX { enabled: face.motion && face.glide; NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
    Behavior on shownY { enabled: face.motion && face.glide; NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
    property real blink: 1          // vertical eye scale; dips to ~0 for a blink
    property bool sleepy: false
    // How open the eyes are when not blinking: heavy-lidded while dozing.
    property real lid: sleepy ? .42 : 1
    Behavior on lid { enabled: face.motion; NumberAnimation { duration: 600; easing.type: Easing.InOutSine } }
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
        const sx = thinking ? .08 : sleepy ? 0 : lookX + glance;
        const sy = thinking ? -.07 : sleepy ? .05 : lookY;
        const tx = quarter(sx * size * reach), ty = quarter(sy * size * reach);
        if (tx === shownX && ty === shownY) return;
        glide = Math.abs(tx - shownX) >= .75 || Math.abs(ty - shownY) >= .75;
        shownX = tx;
        shownY = ty;
    }
    onThinkingChanged: retarget()
    onSleepyChanged: retarget()
    onGlanceChanged: retarget()
    onSizeChanged: retarget()
    onArtChanged: retarget()
    // On another monitor's notch the whole face holds still (no gaze, blinks, glances,
    // dozing or thinking looks), so with a notch on every monitor only the one you're
    // on ever redraws. It picks up again the moment the pointer arrives.
    onPointerAtChanged: {
        if (pointerAt <= 0 || !pointerHere(pointerX, pointerY)) { ponder.stop(); return; }
        updateLook();
        if (sleepy) wake.restart();
        sleepy = false;
        drowsy.restart();
        thinking = false;
        if (working) ponder.restart();
    }
    onWorkingChanged: {
        if (working) { if (attended) ponder.restart(); }
        else { ponder.stop(); if (attended) thinking = false; }
    }
    onVisibleChanged: if (visible && attended) updateLook()

    // Dozes off after three quiet minutes, wakes with a wide-eyed blink.
    Timer { id: drowsy; interval: 180000; running: face.visible && face.attended && !face.working; onTriggered: face.sleepy = true }
    Timer { id: ponder; interval: 1500; onTriggered: face.thinking = face.working && face.attended }
    SequentialAnimation {
        id: wake
        NumberAnimation { target: face; property: "blink"; to: 1.25; duration: 120; easing.type: Easing.OutQuad }
        NumberAnimation { target: face; property: "blink"; to: 1; duration: 220; easing.type: Easing.InOutQuad }
    }
    SequentialAnimation {
        id: blinkOnce
        NumberAnimation { target: face; property: "blink"; to: .08; duration: 70; easing.type: Easing.InQuad }
        PauseAnimation { duration: 50 }
        NumberAnimation { target: face; property: "blink"; to: 1; duration: 130; easing.type: Easing.OutQuad }
    }
    Timer {
        id: blinker
        interval: 3000
        running: face.visible && face.attended && face.motion && !face.sleepy && !face.happy
        onTriggered: {
            blinkOnce.restart();
            const resting = Date.now() - face.pointerAt > 4000;
            // Now and then, with the pointer resting, the eyes wander to one side and back.
            if (resting && !face.thinking && Math.random() < .25) {
                face.glance = (Math.random() < .5 ? -1 : 1) * .09;
                unglance.restart();
            }
            face.blinked(resting);
            interval = 2600 + Math.random() * 3800;
            restart();
        }
    }
    Timer { id: unglance; interval: 1300; onTriggered: face.glance = 0 }

    Loader {
        id: artLoader
        anchors.fill: parent
        function load() { setSource(Qt.resolvedUrl(face.characters[face.shownCharacter]), { face: face }); }
        Component.onCompleted: load()
        Connections { target: face; function onShownCharacterChanged() { artLoader.load(); } }
    }
}
