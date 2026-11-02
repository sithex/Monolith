exports.fallback = Object.freeze({name: "unavailable", background: "#111111", accent: "#dddddd", text: "#dddddd", primary: "#eeeeee", dim: "#999999", border: "#444444", surface: "#222222", error: "#ff8888"});
exports.terminalTheme = (palette) => ({background: palette.background, foreground: palette.text, cursor: palette.accent, selectionBackground: palette.border});
