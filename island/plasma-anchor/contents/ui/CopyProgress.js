.pragma library
// File-copy progress for the notch: the island's `CopyProgress(i)` signal argument or the reply
// of its `Progress()` method. Plasma's D-Bus module may hand either over as a plain number, a
// wrapper object ({value}), a list, or a pending reply ({value, values, isError}); anything
// that is not 0..100 means "none".
function percent(raw) {
    var v = raw;
    for (var i = 0; i < 6 && v !== null && v !== undefined && typeof v === "object"; i++) {
        if (v.isError === true) return -1;
        if (Array.isArray(v) || (typeof v.length === "number" && v.length > 0)) v = v[0];
        else if (v.value !== undefined) v = v.value;
        else if (v.values !== undefined) v = v.values;
        else break;
    }
    if (v === null || v === undefined || v === "" || typeof v !== "number" && typeof v !== "string") return -1;
    var n = Number(v);
    return isFinite(n) && n >= 0 && n <= 100 ? Math.round(n) : -1;
}
