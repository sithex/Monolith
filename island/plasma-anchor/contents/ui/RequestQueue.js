// A single executable source at a time preserves GUI event order and prevents
// Plasma's same-source coalescing from losing a rapid re-entry.
function create(send) {
    var pending = [];
    var active = false;
    function next() {
        if (active || pending.length === 0) return;
        active = true;
        send(pending.shift());
    }
    return {
        enqueue: function(command) {
            if (command === "status" && pending.indexOf(command) !== -1) return;
            pending.push(command);
            next();
        },
        complete: function() { active = false; next(); }
    };
}
