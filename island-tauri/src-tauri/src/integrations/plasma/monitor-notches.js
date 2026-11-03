// Monolith "notch on every monitor". Run by the island ONLY at startup and when the setting
// (or notch width) changes: never periodically, never on a click, never retried.
//
// Safety rules (see the ripple-island plasma-multimonitor pitfalls):
//  * It NEVER creates panels. A screen without an existing top panel is reported, not "fixed".
//  * It only adds/removes notch widgets it marked itself; user widgets are never touched.
//  * It prints a JSON report read back from Plasma, so the caller can see where each notch
//    actually landed instead of trusting the request.
(function(enabled, style) {
    var kind = "local.hermes.island.notch3", owner = "monolith-monitor-v2";
    var kinds = [kind, "local.hermes.island.notch2", "local.hermes.island.notch"];
    function isNotch(w) { return kinds.indexOf(w.type) >= 0; }
    function owned(w) { w.currentConfigGroup = ["General"]; return w.readConfig("monolithMonitorOwner", "") === owner; }
    var report = {enabled: enabled, added: [], removed: [], skipped: [], ordered: [], notches: []};
    var ps = panels();
    if (!enabled) {
        ps.forEach(function(p) {
            p.widgets().filter(function(w) { return isNotch(w) && owned(w); }).forEach(function(w) {
                report.removed.push({panel: p.id, widget: w.id, screen: p.screen});
                w.remove();
            });
        });
    } else {
        for (var screen = 0; screen < screenCount; screen++) {
            var top = ps.filter(function(p) { return p.screen === screen && p.location === "top"; });
            if (top.length === 0) { report.skipped.push({screen: screen, why: "no top panel"}); continue; }
            if (top.some(function(p) { return p.widgets().some(isNotch); })) continue;
            var p = top[0];
            var w = p.addWidget(kind);
            if (!w) { report.skipped.push({screen: screen, why: "widget unavailable"}); continue; }
            w.currentConfigGroup = ["General"];
            w.writeConfig("monolithMonitorOwner", owner);
            w.writeConfig("notchAppearance", JSON.stringify(style));
            w.reloadConfig();
            report.added.push({panel: p.id, widget: w.id, screen: screen});
        }
        // Centre slot for OUR notches only: just before the panel's centred clock (the clock is
        // left alone), else after the first spacer. Plasma appends a new applet to AppletOrder
        // after this script returns and reads the order only when a panel loads, so the caller
        // runs the script a second time after additions; the move shows on Plasma's next load.
        panels().forEach(function(p) {
            p.widgets().filter(function(w) { return w.type === kind && owned(w); }).forEach(function(w) {
                p.currentConfigGroup = ["General"];
                var order = String(p.readConfig("AppletOrder", "")).split(";").filter(function(x) { return x && x !== String(w.id); });
                var widgets = p.widgets();
                var clock = widgets.filter(function(x) { return x.type === "org.kde.plasma.digitalclock"; })[0];
                var spacer = widgets.filter(function(x) { return x.type === "org.kde.plasma.panelspacer"; })[0];
                var at = clock ? order.indexOf(String(clock.id)) : spacer ? order.indexOf(String(spacer.id)) + 1 : -1;
                if (at < 0 || !order.length) return;
                order.splice(at, 0, String(w.id));
                var next = order.join(";");
                if (String(p.readConfig("AppletOrder", "")) !== next) {
                    p.writeConfig("AppletOrder", next);
                    report.ordered.push({panel: p.id, widget: w.id});
                }
            });
        });
    }
    // Read back what Plasma actually holds now.
    panels().forEach(function(p) {
        p.widgets().filter(isNotch).forEach(function(w) {
            report.notches.push({panel: p.id, widget: w.id, screen: p.screen, location: p.location, owned: owned(w)});
        });
    });
    print(JSON.stringify(report));
})(__ENABLED__, __STYLE__);
