import "../plasma-anchor/contents/ui" as Panel
import QtQuick
import QtTest

TestCase {
    id: suite
    name: "CompanionRevision"
    width: 220; height: 220; visible: true; when: windowShown
    Component {
        id: factory
        Panel.Companion {
            x: 80; y: 80; width: 20; height: 20
            character: "companioncube"
            screenArea: Qt.rect(-2000,-2000,4000,4000)
        }
    }
    function point(face, dx, dy) {
        const c = face.mapToGlobal(10,10);
        face.pointerX = c.x + dx;
        face.pointerY = c.y + dy;
        face.pointerAt = Math.max(Date.now(), face.pointerAt + 1);
        wait(210);
    }
    function pose(a) { return [a.shiftX,a.shiftY,a.tilt]; }
    function test_nurgling_beady_gaze_blink_and_reaction() {
        const f=createTemporaryObject(factory,suite,{character:"nurgling"}), a=f.art;
        point(f,0,0);
        const eye=findChild(f,"companionEye0"), arm=findChild(a,"scratchArm");
        compare(eye.width,1,"beady 1px eyes preserve room for broad grin");
        verify(arm !== null);
        const normalEyeX=eye.x;
        point(f,900,0); verify(eye.x>normalEyeX);
        f.blink=0.08; verify(eye.height<1);
        f.blink=1;
        const normalY=arm.y;
        f.happy=true;
        verify(findChild(a,"hoverMark").visible);
        verify(arm.y<normalY);
        f.happy=false;
        a.lastHabitAt=0;
        f.blinked(true);
        verify(a.habitRunning);
        wait(160); verify(arm.y<normalY);
        tryCompare(a,"habitRunning",false,1100);
        compare(arm.y,normalY);
    }
    function test_cube_pointer_tracking() {
        const f = createTemporaryObject(factory, suite), a = f.art;
        point(f, -900, 0);
        verify(a.shiftX < 0, "real pointer left must translate Cube");
        verify(a.tilt < 0);
        point(f,900,0); verify(a.shiftX > 0); verify(a.tilt > 0);
        point(f,0,-900); verify(a.shiftY < 0);
        point(f,0,900); verify(a.shiftY > 0);
        point(f,0,0); compare(pose(a),[0,0,0]);
        let changes = 0;
        function count() { changes++; }
        a.shiftXChanged.connect(count); a.shiftYChanged.connect(count); a.tiltChanged.connect(count);
        for (let i=0;i<12;i++) {
            // A changed relay timestamp and sub-quantum real pointer jitter.
            point(f, (i%2)*0.01, 0.01);
        }
        compare(changes,0,"duplicate snapped inputs must not change art transforms");
        a.shiftXChanged.disconnect(count); a.shiftYChanged.disconnect(count); a.tiltChanged.disconnect(count);
    }
    function test_cube_freeze_data() {
        return ["visible","artVisible","motion","attended","sleepy"].map(g => ({tag:g,gate:g}));
    }
    function test_cube_freeze(data) {
        const f = createTemporaryObject(factory,suite), a=f.art;
        point(f,900,900);
        a.lastHabitAt=0; a.startHabit(); wait(150);
        if (data.gate === "artVisible") a.visible=false;
        else if (data.gate === "attended") f.screenArea=Qt.rect(5000,5000,10,10);
        else f[data.gate]=data.gate === "sleepy";
        const held=pose(a);
        if(data.gate === "sleepy") {
            // Real pointer wakes the brain; change its existing target instead.
            f.glance=-0.1;
            wait(300);
        } else point(f,-900,-900);
        compare(pose(a),held, "inactive pose held: "+data.gate);
        compare(a.habitRunning,false);
        compare(findChild(a,"habitTick").running,false);
    }
    function test_cube_bounds_data() {
        return [1,1.25,1.5,2].map(s => ({tag:String(s),scale:s}));
    }
    function test_cube_bounds(data) {
        const f=createTemporaryObject(factory,suite,{scale:data.scale}), a=f.art;
        const p=findChild(a,"cubePose"); verify(p !== null);
        for(const x of [-900,0,900]) for(const y of [-900,0,900]) {
            point(f,x,y);
            for(const happy of [false,true]) {
                f.happy=happy;
                for(const frame of [0,1,2,3,4,5,6]) {
                    a.habitFrame=frame;
                    verify(Math.abs(p.rotation)<=8);
                    compare(p.rotation%4,0);
                    compare(a.shiftX/a.pixel,Math.round(a.shiftX/a.pixel));
                    compare(a.shiftY/a.pixel,Math.round(a.shiftY/a.pixel));
                    for(const c of [[2,2],[18,2],[2,18],[18,18]]) {
                        const v=p.mapToItem(f,c[0],c[1]);
                        verify(v.x >= 0 && v.y >=0 && v.x<=20 && v.y<=20,JSON.stringify(v));
                    }
                }
            }
        }
        f.character="blob"; wait(250);
        compare(f.shownCharacter,"blob");
    }
}
