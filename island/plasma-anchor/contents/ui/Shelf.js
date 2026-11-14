.pragma library
// Drop shelf, notch side. Only local `file://` URLs are taken (at most 64 per drop, as the
// island accepts); anything else is refused so the panel keeps its own drop behaviour.
// `String(url)` is QUrl's pretty form; the island percent-decodes and validates every path.
var MAX = 64;
function fileUrls(urls) {
    if (!urls || typeof urls.length !== "number" || urls.length === 0 || urls.length > MAX) return [];
    var out = [];
    for (var i = 0; i < urls.length; i++) {
        var s = String(urls[i]);
        if (s.indexOf("file:///") !== 0 && s.indexOf("file://localhost/") !== 0) return [];
        out.push(s);
    }
    return out;
}
// The `Add` reply: how many were parked. Plasma's D-Bus module may hand it over as a number,
// a wrapper ({value}), a list, or a pending reply ({value, values, isError}).
function count(raw) {
    var v = raw;
    for (var i = 0; i < 6 && v !== null && v !== undefined && typeof v === "object"; i++) {
        if (v.isError === true) return 0;
        if (Array.isArray(v) || (typeof v.length === "number" && v.length > 0)) v = v[0];
        else if (v.value !== undefined) v = v.value;
        else if (v.values !== undefined) v = v.values;
        else break;
    }
    var n = Number(v);
    return isFinite(n) && n > 0 ? Math.floor(n) : 0;
}
