import QtQml
import org.kde.plasma.workspace.dbus as DBus
import "Shelf.js" as Shelf

// Drop shelf (Settings → shelfEnabled): hands dropped file URLs to the island's
// `/Shelf local.hermes.Monolith.Shelf.Add(as) → u` on its own bus name. D-Bus instead of the
// helper command line, so no shell ever sees a file name. No `signature` field: Plasma's
// module rejects calls that carry one. `done(count)` gets how many were parked (0 = none).
QtObject {
    id: sink
    function add(urls, done) {
        var list = Shelf.fileUrls(urls);
        if (!list.length) { if (done) done(0); return; }
        DBus.SessionBus.asyncCall({service: "local.hermes.Monolith", path: "/Shelf",
            iface: "local.hermes.Monolith.Shelf", member: "Add", arguments: [list]},
            function(reply) { if (done) done(Shelf.count(reply)); },
            function(error) { if (done) done(0); });
    }
}
