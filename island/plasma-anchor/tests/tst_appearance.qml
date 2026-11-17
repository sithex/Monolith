import QtQuick
import QtTest
import "../contents/ui" as Panel

TestCase {
    name: "NotchAppearance"
    Panel.NotchAppearance { id: state }

    function ready(accent) {
        return { appearanceReady: true, accent: accent, background: "#123456", text: "#abcdef", dim: "#998877",
                 notchBorder: { color: "#102030", opacity: 0, width: 4 }, notchWidth: 300, companion: false };
    }
    function init() { state.value = ({}); }

    function test_cold_defaults_are_unchanged() {
        compare(String(state.accent), "#f2cc60");
        compare(String(state.background), "#171717");
        compare(String(state.text), "#f1ede4");
        compare(String(state.dim), "#aaa69d");
        state.accept({ appearanceReady: false, accent: null, background: null, text: null, dim: null, notchWidth: 280 });
        compare(String(state.accent), "#f2cc60");
        compare(String(state.background), "#171717");
        compare(String(state.text), "#f1ede4");
        compare(String(state.dim), "#aaa69d");
        compare(state.value.notchWidth, 280); // cold status still supplies the old non-colour defaults/settings
    }
    function test_not_ready_and_incomplete_retain_the_whole_previous_appearance() {
        const previous = ready("#00ff00");
        state.accept(previous);
        const replies = [ { appearanceReady: false }, { appearanceReady: true },
            Object.assign(ready("#ff0000"), { appearanceReady: false }),
            Object.assign(ready("#ff0000"), { text: null }),
            Object.assign(ready("#ff0000"), { dim: "red" }),
            Object.assign(ready("#ff0000"), { notchBorder: {} }), null ];
        for (const reply of replies) {
            state.accept(reply);
            compare(state.value, previous);
            compare(state.value.notchWidth, 300);
            compare(state.value.companion, false);
            compare(state.value.notchBorder.opacity, 0);
        }
        state.accept(ready("#0000ff"));
        compare(String(state.accent), "#0000ff");
    }
    function test_complete_legacy_persisted_appearance_is_retained_after_restart() {
        const previous = ready("#654321");
        delete previous.appearanceReady;
        state.accept(previous); // persisted KConfig from an older island
        state.accept({ appearanceReady: false, accent: null }); // early status reply
        compare(state.value, previous);
        compare(String(state.accent), "#654321");
    }
    function test_cold_incomplete_legacy_appearance_keeps_its_old_fallbacks() {
        state.accept({ accent: "#334455", notchWidth: 160 });
        compare(String(state.accent), "#334455");
        compare(String(state.background), "#171717");
        compare(String(state.text), "#f1ede4");
        compare(String(state.dim), "#aaa69d");
    }
}
