//! Drop shelf, pure part (`integrations::shelf::core`): URI validation, encoding, the list.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hermes_island_lib::integrations::shelf::core::*;
use std::{
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[test]
fn shelf_uris_decode_to_exact_bytes_in_pretty_and_encoded_forms() {
    // QUrl's pretty form (what `String(url)` gives in QML): space and ü literal, `'` encoded.
    assert_eq!(
        parse_uri("file:///data/user/we ird %27%22%24%28id%29ü.txt").unwrap(),
        PathBuf::from("/data/user/we ird '\"$(id)ü.txt")
    );
    assert_eq!(
        parse_uri("file://localhost/tmp/a%20b").unwrap(),
        PathBuf::from("/tmp/a b")
    );
    assert_eq!(parse_uri("FILE:///x").unwrap(), PathBuf::from("/x"));
    // A name that is not UTF-8 survives (wry's lossy decoder would corrupt it).
    let raw = parse_uri("file:///tmp/%FFname").unwrap();
    assert_eq!(raw.as_os_str().as_bytes(), b"/tmp/\xffname");
}

#[test]
fn shelf_uris_refuse_everything_outside_the_strict_rules() {
    let cases: &[(&str, Reject)] = &[
        ("http://example.com/a", Reject::NotFileScheme),
        ("file:/tmp/a", Reject::NotFileScheme),
        ("/tmp/a", Reject::NotFileScheme),
        ("file://server/share/a", Reject::RemoteHost),
        ("file://localhost.evil/a", Reject::RemoteHost),
        ("file://", Reject::NotAbsolute),
        ("file:///tmp/a?x=1", Reject::QueryOrFragment),
        ("file:///tmp/a#frag", Reject::QueryOrFragment),
        ("file:///tmp/%zz", Reject::BadEscape),
        ("file:///tmp/%4", Reject::BadEscape),
        ("file:///tmp/%+1", Reject::BadEscape),
        ("file:///tmp/../etc/passwd", Reject::ParentDir),
        ("file:///tmp/%2E%2E/etc", Reject::ParentDir),
        ("file:///tmp/a%00b", Reject::Nul),
        ("file:///tmp/line%0Abreak.txt", Reject::ControlCharacter),
        ("file:///tmp/tab%09", Reject::ControlCharacter),
        ("file:///tmp/del%7F", Reject::ControlCharacter),
        ("file:///tmp/c1%C2%85", Reject::ControlCharacter),
    ];
    for (uri, why) in cases {
        assert_eq!(parse_uri(uri), Err(*why), "{uri}");
    }
    let long = format!("file:///{}", "a".repeat(MAX_URI_BYTES));
    assert_eq!(parse_uri(&long), Err(Reject::TooLong));
    let fits = format!("file:///{}", "a".repeat(MAX_URI_BYTES - 8));
    assert!(parse_uri(&fits).is_ok());
}

#[test]
fn shelf_accepts_only_existing_plain_files_and_folders() {
    let kind = |k: Option<Kind>| move |_: &Path| k;
    assert_eq!(
        accept("file:///a", kind(Some(Kind::File))).unwrap().1,
        Kind::File
    );
    assert_eq!(
        accept("file:///a", kind(Some(Kind::Folder))).unwrap().1,
        Kind::Folder
    );
    assert_eq!(accept("file:///a", kind(None)), Err(Reject::Missing));
    assert_eq!(
        accept("file:///a", kind(Some(Kind::Symlink))),
        Err(Reject::Symlink)
    );
    assert_eq!(
        accept("file:///a", kind(Some(Kind::Other))),
        Err(Reject::NotFileOrFolder)
    );
}

#[test]
fn shelf_encoding_round_trips_any_name() {
    for bytes in [
        &b"/data/user/plain.txt"[..],
        b"/tmp/we ird '\"$(id)\xc3\xbc.txt",
        b"/tmp/\xff\xfe",
        b"/tmp/100%/a#b?c",
    ] {
        let path = Path::new(OsStr::from_bytes(bytes));
        let uri = encode_uri(path);
        assert!(uri.is_ascii(), "{uri}");
        assert!(!uri.contains(['#', '?', ' ']), "{uri}");
        assert_eq!(parse_uri(&uri).unwrap(), path, "{uri}");
    }
}

#[test]
fn shelf_list_is_newest_first_unique_and_capped() {
    let p = |s: &str| PathBuf::from(s);
    let mut shelf = Shelf::default();
    assert!(shelf.add(vec![p("/a"), p("/b")], 1, 3));
    assert_eq!(
        shelf.items().iter().map(|i| i.path.clone()).collect::<Vec<_>>(),
        [p("/a"), p("/b")]
    );
    assert!(shelf.add(vec![p("/c"), p("/d")], 2, 3));
    // Oldest dropped past the cap; the drop's own order kept.
    assert_eq!(
        shelf.items().iter().map(|i| i.path.clone()).collect::<Vec<_>>(),
        [p("/c"), p("/d"), p("/a")]
    );
    // A path already parked moves to the front instead of appearing twice.
    assert!(shelf.add(vec![p("/a")], 3, 3));
    assert_eq!(shelf.items()[0], Item { path: p("/a"), added_ms: 3 });
    assert_eq!(shelf.items().len(), 3);
    assert!(shelf.trim(1));
    assert_eq!(shelf.items().len(), 1);
    assert!(shelf.remove(&p("/a")));
    assert!(!shelf.remove(&p("/a")));
    assert!(!shelf.clear());
    assert_eq!(clamp_max(0.0), 1);
    assert_eq!(clamp_max(99.0), 50);
    assert_eq!(clamp_max(f64::NAN), 12);
}

#[test]
fn shelf_persists_as_uris_and_skips_entries_that_no_longer_validate() {
    let mut shelf = Shelf::default();
    let odd = PathBuf::from(OsStr::from_bytes(b"/tmp/\xffodd name"));
    shelf.add(vec![odd.clone(), PathBuf::from("/tmp/b")], 42, 12);
    let json = shelf.to_json();
    assert!(json.is_ascii());
    assert_eq!(Shelf::from_json(&json, 12), shelf);
    assert_eq!(Shelf::from_json(&json, 1).items().len(), 1);
    let tampered = r#"{"version":1,"items":[{"uri":"file:///tmp/a%0A","addedMs":1},{"uri":"http://x/y","addedMs":1},{"uri":"file:///ok","addedMs":7},{"uri":"file:///ok","addedMs":8},{"addedMs":1}]}"#;
    let loaded = Shelf::from_json(tampered, 12);
    assert_eq!(loaded.items(), [Item { path: PathBuf::from("/ok"), added_ms: 7 }]);
    assert!(Shelf::from_json("not json", 12).items().is_empty());
    assert!(Shelf::from_json(r#"{"version":2,"items":[{"uri":"file:///ok","addedMs":7}]}"#, 12).items().is_empty());
}

#[test]
fn shelf_launcher_app_is_appended_to_a_saved_v2_registry_last() {
    use hermes_island_lib::{
        contract::{island::IslandView, modules::Registry},
        services::workspace::module_route,
    };
    // A saved registry from before the shelf: only some modules, with gaps in `order`.
    let saved = r#"{"version":2,"modules":[
        {"id":"terminal","enabled":true,"order":0,"config":{"type":"terminal","fontSize":13}},
        {"id":"settings","enabled":true,"order":5,"config":{"type":"settings"}},
        {"id":"downloads","enabled":false,"order":9,"config":{"type":"downloads"}}]}"#;
    let (registry, _) = Registry::from_json(saved).unwrap();
    let ids: Vec<_> = registry.modules().iter().map(|m| (m.id.as_str(), m.order, m.enabled)).collect();
    assert_eq!(
        ids,
        // (`from_json` normalizes orders as before; the user's relative order is kept.)
        [("terminal", 0, true), ("settings", 1, true), ("downloads", 2, false), ("shelf", 3, true)]
    );
    // Already there (even disabled by the user): left exactly as saved.
    let off = saved.replace(
        r#"{"type":"downloads"}}]"#,
        r#"{"type":"downloads"}},{"id":"shelf","enabled":false,"order":10,"config":{"type":"shelf"}}]"#,
    );
    let (kept, _) = Registry::from_json(&off).unwrap();
    assert_eq!(kept.modules().len(), 4);
    assert!(!kept.modules()[3].enabled);
    assert!(Registry::defaults().modules().iter().any(|m| m.id.as_str() == "shelf" && m.enabled));
    assert_eq!(module_route("shelf"), Some(IslandView::Shelf));
}
