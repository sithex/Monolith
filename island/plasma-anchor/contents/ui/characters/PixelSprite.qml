import QtQuick

// Crisp pixel art from text rows: each character is one sprite pixel (one logical px),
// looked up in `colors`; anything not in `colors` (e.g. ".") is transparent. Drawn as one
// unantialiased Rectangle per horizontal run, so at 100% scale every pixel lands exactly
// on a screen pixel. Static: the runs are rebuilt only when `rows` or `colors` change,
// so swap sprites for rare state changes (hover, sleep, habit frames), never per tick.
// Move whole sprites with integer x/y instead.
Item {
    id: sprite
    property var rows: []
    property var colors: ({})
    implicitWidth: rows.reduce((w, r) => Math.max(w, r.length), 0)
    implicitHeight: rows.length
    width: implicitWidth
    height: implicitHeight

    readonly property var runs: build(rows, colors)
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
