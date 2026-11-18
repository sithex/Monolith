import QtQuick

// Isaac (The Binding of Isaac): two constantly streaming eyes, dry on hover.
// Keep the custom Item: the two streams share the pill clock, not the shot's burst.
Item {
    id: art
    property Item face
    readonly property real preferredSize: 26
    readonly property real gazeScale: 1
    readonly property bool dry: !!face && face.happy
    readonly property real open: face ? face.blink * face.lid : 1
    readonly property bool dozing: !!face && (face.sleepy || face.lid < .65)
    readonly property int pupilX: gazePx(face ? face.shownX : 0, -2, 2)
    readonly property int pupilY: gazePx(face ? face.shownY : 0, -1, 1)
    function gazePx(value, low, high) {
        return Math.max(low, Math.min(high, Math.round(value)));
    }

    readonly property var colors: ({
        K: "#140606", L: "#f3dbd8", S: "#e3c6c5", R: "#cf9c9b", D: "#b97371",
        E: "#382025", B: "#57434a", W: "#ffffff", P: "#e5a5a3",
        A: "#68989c", T: "#9cc8cc", C: "#d7f5f5"
    })

    PixelSprite {
        objectName: "isaacBody"
        y: 19
        colors: art.colors
        rows: [
            ".......KDRRRRRRRRDK.......",
            "......KRSSSRRSSSSSRK......",
            "......KSKSSSSSSSSKSK......",
            ".......KRSRSSSSRSRK.......",
            "........KKRSRRRSDK........",
            ".........KSSKKRSK.........",
            ".........KKK..KKK........."
        ]
    }
    PixelSprite {
        objectName: "companionBody"
        colors: art.colors
        rows: [
            "........KKKKKKKKKK........",
            ".....KKKLLLLLLLLLLKKK.....",
            "....KLLLLLLLLLSSSSSSSK....",
            "...KLLLLLLSSSSSSSSSSSSK...",
            "..KLLLLSSSSSSSSSSSSSSSSK..",
            "..KLLSSSSSSSSSSSSSSSSSRK..",
            ".KLSSSSSSSSSSSSSSSSSSSSRK.",
            ".KLSSSSSSSSSSSSSSSSSSSSRK.",
            ".KSSSSSSSSSSSSSSSSSSSSSRK.",
            ".KSSSSSSSSSSSSSSSSSSSSSRK.",
            ".KRSSSSSSSSSSSSSSSSSSSSRK.",
            ".KRSSSSSSSSSSSSSSSSSSSRRK.",
            ".KRSSSSSSSSSSSSSSSSSSSRRK.",
            ".KRRSSSSSSSSSSSSSSSSSRRDK.",
            "..KRSSSSSSSSSSSSSSSSSRDK..",
            "...KRSSSSSSSSSSSSSSSSRK...",
            "...KDRSSSSSSSSSSSSSSRDK...",
            "....KDRSSSSSSSSSSSSDRK....",
            "......KKRRRSSSSRRDKK......",
            "........KKKKKKKKKK........"
        ]
    }

    Repeater {
        id: streams
        model: [5, 15]
        PixelSprite {
            id: stream
            required property int modelData
            required property int index
            objectName: "isaacStream"
            x: modelData + 1 + art.pupilX
            y: 12 + art.pupilY
            opacity: art.dry ? 0 : 1
            visible: !art.dry
            colors: art.colors
            rows: [".TA.", "ATTA", "ATTA", "ATTA", "ATTA", "ATTA", "ATTA", ".TA."]

            // Only these tiny overlays move. Keep the channel itself static and put
            // both crests on the same eight-phase clock, with different start phases.
            Rectangle {
                id: crest
                objectName: "isaacFlow"
                visible: art.flowing
                x: 1
                y: (art.flowFrame + 8 - 2 * stream.index) % art.flowSteps
                width: y === 0 || y === 7 ? 1 : 2
                height: 1
                color: art.colors.C
            }
            Rectangle {
                visible: art.flowing
                x: 0
                y: (crest.y + 1) % art.flowSteps
                width: 1
                height: 1
                color: art.colors.T
                opacity: y === 0 || y === 7 ? 0 : 1
            }
            Rectangle {
                visible: art.flowing
                x: 1
                y: (crest.y + 4) % art.flowSteps
                width: 1
                height: 1
                color: art.colors.C
            }
        }
    }
    Repeater {
        model: [5, 15]
        PixelSprite {
            required property int modelData
            required property int index
            objectName: "companionEye" + index
            x: modelData + art.pupilX
            y: 8 + art.pupilY
            colors: art.colors
            rows: art.open < .3 ? ["......", "......", "......", ".E..E.", "..EE..", "......"]
                : art.dozing ? ["......", "......", ".EDDE.", "EKKKKE", "KKBKBK", ".KBBK."]
                : [".EKKK.", "EWKKKE", "KWWKKK", "KKKKKK", "KKKKKB", ".KBBK."]
        }
    }
    PixelSprite {
        objectName: "isaacMouth"
        x: 10 + art.pupilX
        y: 14
        opacity: art.dry ? 0 : 1
        visible: !art.dry
        colors: art.colors
        rows: [".KKKK.", "KEEEEK", "KWWWWK", ".KKKK."]
    }
    PixelSprite {
        objectName: "hoverMark"
        x: 5 + art.pupilX
        y: 13 + art.pupilY
        visible: art.dry
        colors: art.colors
        rows: ["PP............PP"]
        PixelSprite {
            objectName: "isaacSmile"
            x: 5
            y: 1 - art.pupilY
            opacity: art.dry ? 1 : 0
            colors: art.colors
            rows: ["K....K", ".KKKK."]
        }
    }

    // Flow still rides the one 125 ms pill clock (also used by working and music).
    property bool flowing: true
    readonly property int flowSteps: 8
    property int flowFrame: 0
    readonly property bool active: !!face && visible && face.visible && face.attended
        && face.motion && !face.happy && !face.sleepy
    readonly property bool clockWanted: flowing && active && !face.settled && !face.frozen
    readonly property bool canShoot: active && !face.frozen && !face.thinking && !face.waiting
    Connections {
        target: art.face
        function onFrameChanged() {
            if (art.clockWanted) art.flowFrame = (art.flowFrame + 1) % art.flowSteps;
        }
        function onBlinked(resting) {
            if (resting && art.canShoot && Math.random() < .3) art.shoot();
        }
    }

    // Alternate slow drip: one bead follows the stream in whole pixels, then falls away.
    property int dripEye: 0
    property real dripAt: 0
    Timer {
        id: dripper
        objectName: "isaacDripper"
        interval: 3500
        repeat: true
        running: !art.flowing && art.active && !art.face.settled && !art.face.frozen
        onRunningChanged: {
            if (!running) dripAnim.stop();
        }
        onTriggered: {
            if (!dripAnim.running) {
                art.dripEye = Math.random() < .5 ? 0 : 1;
                dripAnim.start();
            }
            interval = 3500 + Math.random() * 2500;
        }
    }
    NumberAnimation {
        id: dripAnim
        target: art
        property: "dripAt"
        from: 0
        to: 1
        duration: 900
        easing.type: Easing.InQuad
    }
    PixelSprite {
        objectName: "isaacDrip"
        // Use the same origin formula as the streams; itemAt() during construction
        // can return null before Repeater has instantiated the first eye.
        x: 6 + 10 * art.dripEye + art.pupilX
        y: 12 + art.pupilY + Math.round(art.dripAt * 9)
        visible: dripper.running && dripAnim.running
        colors: art.colors
        rows: [".CA.", "ATTA", ".TA."]
    }

    // A short, stepped shot from the eye on the gaze side. Only the projectile and
    // splash leave the slot, toward the rounded left edge or the gap before the clock.
    property int habitFrame: 0
    readonly property int habitFrames: 5
    property int shotDir: 1
    property int shotStartX: 18
    property int shotStartY: 12
    readonly property int landX: shotDir > 0 ? 31 : 0
    function shoot() {
        if (!canShoot || habitFrame !== 0) return;
        shotDir = face.shownX < 0 ? -1 : 1;
        shotStartX = (shotDir > 0 ? 18 : 8) + pupilX;
        shotStartY = 12 + pupilY;
        habitFrame = 1;
        shotTick.start();
    }
    function resetHabit() {
        shotTick.stop();
        habitFrame = 0;
    }
    onCanShootChanged: {
        if (!canShoot) resetHabit();
    }
    Timer {
        id: shotTick
        interval: 100
        repeat: true
        onTriggered: {
            if (art.habitFrame >= art.habitFrames) art.resetHabit();
            else art.habitFrame++;
        }
    }
    PixelSprite {
        objectName: "isaacShot"
        visible: art.habitFrame >= 1 && art.habitFrame <= 3
        x: Math.round(art.shotStartX + (art.landX - art.shotStartX) * (art.habitFrame - 1) / 3) - 2
        y: art.shotStartY + (art.habitFrame === 2 ? -1 : art.habitFrame === 3 ? 2 : 0) - 2
        colors: art.colors
        rows: [".AA.", "ACTA", "ATTA", ".AA."]
    }
    PixelSprite {
        objectName: "isaacSplash"
        visible: art.habitFrame >= 4
        x: art.landX - 3
        y: 19
        colors: art.colors
        rows: art.habitFrame === 4 ? [".C...C.", "..TCT..", "..AAA.."]
            : [".......", "T.....T", ".A...A."]
    }
}
