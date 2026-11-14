//! Drop shelf end to end on a private, scratch-only bus: a notch-like client calls
//! `/Shelf local.hermes.Monolith.Shelf.Add(as)` on the island's bus name, served on the same
//! connection as `/Companion`, and the store validates, parks, persists and opens.
//! Never the user's session bus.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus::PrivateBus;
use hermes_island_lib::{
    contract::apps::ShelfOp,
    error::IslandError,
    integrations::{downloads::Opener, plasma::companion::CompanionBus, shelf::ShelfStore},
};
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::{symlink, DirBuilderExt, PermissionsExt},
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::async_runtime::block_on;

#[derive(Default)]
struct Recorder(Mutex<Vec<Vec<OsString>>>);
impl Opener for Recorder {
    fn open(&self, argv: &[OsString]) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(argv.to_vec());
        Ok(())
    }
}

fn add(conn: &zbus::Connection, uris: Vec<String>) -> u32 {
    block_on(conn.call_method(
        Some("local.hermes.Monolith"),
        "/Shelf",
        Some("local.hermes.Monolith.Shelf"),
        "Add",
        &(uris,),
    ))
    .unwrap()
    .body()
    .deserialize::<u32>()
    .unwrap()
}

/// What QML's `String(url)` hands over: the pretty form, with `'`, `"`, `$`, `(` and `)`
/// percent-encoded and spaces and non-ASCII literal (the Part A spike's observation).
fn pretty(path: &Path) -> String {
    let mut out = String::from("file://");
    for c in path.to_str().unwrap().chars() {
        match c {
            '\'' | '"' | '$' | '(' | ')' | '\n' | '%' | '#' | '?' => {
                let mut buf = [0; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

#[test]
fn shelf_add_over_dbus_validates_parks_persists_and_opens_by_id() {
    let bus = PrivateBus::start("shelf");
    let address = bus.address();
    let home = bus.root.join("shelf-home");
    let data = bus.root.join("shelf-data");
    fs::DirBuilder::new().mode(0o700).create(&home).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&data).unwrap();
    let hostile = home.join("we ird '\"$(id)ü.txt");
    let folder = home.join("Folder");
    let newline = home.join("line\nbreak.txt");
    let link = home.join("link.txt");
    fs::write(&hostile, b"x").unwrap();
    fs::write(&newline, b"x").unwrap();
    fs::create_dir(&folder).unwrap();
    symlink(&hostile, &link).unwrap();

    let opener = Arc::new(Recorder::default());
    let store = ShelfStore::new(&data, opener.clone());
    store.configure(true, 3.0);
    let island = block_on(CompanionBus::start_at(&address, Some(store.clone()))).unwrap();
    let notch = block_on(zbus::connection::Builder::address(address.as_str()).build()).unwrap();

    // Both objects live on the one island connection.
    let xml = |path: &str| {
        block_on(notch.call_method(
            Some("local.hermes.Monolith"),
            path,
            Some("org.freedesktop.DBus.Introspectable"),
            "Introspect",
            &(),
        ))
        .unwrap()
        .body()
        .deserialize::<String>()
        .unwrap()
    };
    assert!(xml("/Shelf").contains("local.hermes.Monolith.Shelf"));
    assert!(xml("/Companion").contains("local.hermes.Monolith.Companion"));

    let accepted = add(
        &notch,
        vec![
            pretty(&hostile),
            pretty(&folder),
            pretty(&newline),                                   // control character: refused
            pretty(&link),                                      // symlink: refused
            pretty(&home.join("missing.txt")),                  // gone: refused
            "https://example.com/x".into(),                     // not a file: refused
            format!("file://{}/../data/x", home.display()),     // `..`: refused
        ],
    );
    assert_eq!(accepted, 2, "only the plain file and the folder are parked");
    let now = store.current().unwrap();
    assert_eq!(now.items.len(), 2);
    assert_eq!(now.items[0].path, hostile.to_str().unwrap());
    assert!(now.items[0].present && !now.items[0].folder);
    assert!(now.items[1].folder && now.items[1].name == "Folder");
    assert!(now.items[0].uri.is_ascii() && !now.items[0].uri.contains(' '));

    // Persisted privately, as URIs (no raw name in the file), and reloaded.
    let saved = data.join("shelf.json");
    assert_eq!(fs::metadata(&saved).unwrap().permissions().mode() & 0o777, 0o600);
    let text = fs::read_to_string(&saved).unwrap();
    assert!(!text.contains("$(id)") && text.contains("%24%28id%29"), "{text}");
    let again = ShelfStore::new(&data, opener.clone());
    again.configure(true, 3.0);
    assert_eq!(again.current().unwrap().items, now.items);

    // Open / Reveal by id: the argv comes from the shelf, one argument per path.
    let id = now.items[0].id.clone();
    store.command(ShelfOp::Open { id: id.clone() }).unwrap();
    store.command(ShelfOp::Reveal { id: id.clone() }).unwrap();
    let calls = opener.0.lock().unwrap().clone();
    assert_eq!(calls[0], [OsString::from("xdg-open"), hostile.clone().into_os_string()]);
    assert_eq!(
        calls[1],
        [OsString::from("dolphin"), OsString::from("--select"), hostile.clone().into_os_string()]
    );
    // An id that is not on the shelf opens nothing, even for a real file.
    let stranger = home.join("stranger.txt");
    fs::write(&stranger, b"x").unwrap();
    let foreign = hermes_island_lib::integrations::shelf::core::encode_uri(&stranger);
    assert!(store.command(ShelfOp::Open { id: foreign }).is_err());
    assert_eq!(opener.0.lock().unwrap().len(), 2);

    // A parked file that disappears shows as moved/deleted and cannot be opened.
    fs::remove_file(&hostile).unwrap();
    assert!(!store.current().unwrap().items[0].present);
    assert!(store.command(ShelfOp::Open { id: id.clone() }).is_err());
    store.command(ShelfOp::Remove { id }).unwrap();
    assert_eq!(store.current().unwrap().items.len(), 1);

    // The cap drops the oldest; more than 64 URIs refuses the whole drop.
    for n in 0..4 {
        let f = home.join(format!("f{n}"));
        fs::write(&f, b"x").unwrap();
        assert_eq!(add(&notch, vec![pretty(&f)]), 1);
    }
    let names: Vec<_> = store.current().unwrap().items.into_iter().map(|i| i.name).collect();
    assert_eq!(names, ["f3", "f2", "f1"]);
    let f0 = pretty(&home.join("f0"));
    assert_eq!(add(&notch, vec![f0.clone(); 65]), 0);
    assert_eq!(add(&notch, vec![f0.clone(); 64]), 1);

    // Off switch: the shelf takes nothing, the list stays.
    store.configure(false, 3.0);
    assert_eq!(add(&notch, vec![pretty(&home.join("f1"))]), 0);
    assert!(!store.current().unwrap().enabled);
    assert_eq!(store.current().unwrap().items.len(), 3);
    store.command(ShelfOp::Clear {}).unwrap();
    assert!(store.current().unwrap().items.is_empty());
    drop(island);
}
