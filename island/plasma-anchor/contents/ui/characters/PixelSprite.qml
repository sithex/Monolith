import QtQuick

// Crisp pixel art from text rows: each character is one sprite pixel (one logical px),
// looked up in `colors`; anything not in `colors` (e.g. ".") is transparent. Drawn as one
// unantialiased Rectangle per horizontal run, so at 100% scale every pixel lands exactly
// on a screen pixel. The runs (and so the Rectangles) are replaced only when the drawn
// pixels actually change: a `rows` binding that re-evaluates every animation frame (a
// blink, a doze) but lands on the same picture costs nothing. Still, swap sprites only for
// rare state changes and move whole sprites with integer x/y.
Item {
    id: sprite
    property var rows: []
    property var colors: ({})
    implicitWidth: rows.reduce((w, r) => Math.max(w, r.length), 0)
    implicitHeight: rows.length
    width: implicitWidth
    height: implicitHeight

    property var runs: []
    property string drawn: ""
    function refresh() {
        const next = build(rows, colors);
        const key = next.map(r => r.x + "," + r.y + "," + r.w + "," + r.color).join(";");
        if (key === drawn) return;
        drawn = key;
        runs = next;
    }
    onRowsChanged: refresh()
    onColorsChanged: refresh()
    Component.onCompleted: refresh()
    function build(rows, colors) {
        const out = [];
        for (let y = 0; y < rows.length; y++) {
            const row = rows[y];
            let x = 0;
            while (x < row.length) {
                const c = row[x];
                let end = x + 1;
                while (end < row.length && row[end] === c) end++;
                if (Object.prototype.hasOwnProperty.call(colors, c))
                    out.push({ x: x, y: y, w: end - x, color: colors[c] });
                x = end;
            }
        }
        return out;
    }

    Repeater {
        model: sprite.runs
        Rectangle {
            required property var modelData
            x: modelData.x
            y: modelData.y
            width: modelData.w
            height: 1
            color: modelData.color
            antialiasing: false
        }
    }
}
