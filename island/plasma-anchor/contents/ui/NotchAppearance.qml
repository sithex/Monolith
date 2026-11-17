import QtQuick

// One acceptance path for status replies and persisted KConfig pushes. A cold applet keeps
// the legacy defaults; once complete, its whole appearance survives a cold island restart.
QtObject {
    property var value: ({})
    readonly property color accent: value.accent || "#f2cc60"
    readonly property color background: value.background || "#171717"
    readonly property color text: value.text || "#f1ede4"
    readonly property color dim: value.dim || "#aaa69d"

    function complete(candidate) {
        if (!candidate || candidate.appearanceReady === false) return false;
        const roles = [candidate.accent, candidate.background, candidate.text, candidate.dim,
                       (candidate.notchBorder || {}).color];
        return roles.every(function(role) { return typeof role === "string" && /^#[0-9a-fA-F]{6}$/.test(role); });
    }

    function accept(candidate) {
        // Missing readiness is allowed for complete legacy persisted appearances.
        if (complete(candidate) || !complete(value)) value = candidate || ({});
    }
}
