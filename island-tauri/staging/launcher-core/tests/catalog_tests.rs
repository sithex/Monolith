use launcher_core::{visible_in, AppEntry, Catalog};

fn entry(id: &str, name: &str) -> AppEntry {
    AppEntry {
        desktop_id: id.to_string(),
        name: name.to_string(),
        generic_name: String::new(),
        comment: String::new(),
        icon: String::new(),
        exec: "x".to_string(),
        try_exec: String::new(),
        terminal: false,
        no_display: false,
        hidden: false,
        only_show_in: Vec::new(),
        not_show_in: Vec::new(),
        categories: Vec::new(),
        keywords: Vec::new(),
    }
}

fn hidden_entry(id: &str) -> AppEntry {
    let mut e = entry(id, "H");
    e.hidden = true;
    e
}

#[test]
fn visible_default_is_visible() {
    let e = entry("a", "A");
    assert!(visible_in(&e, &[]));
    assert!(visible_in(&e, &["KDE"]));
}

#[test]
fn visible_hidden_and_no_display() {
    let mut h = entry("a", "A");
    h.hidden = true;
    assert!(!visible_in(&h, &["KDE"]));
    let mut n = entry("a", "A");
    n.no_display = true;
    assert!(!visible_in(&n, &["KDE"]));
}

#[test]
fn visible_only_show_in_intersects() {
    let mut e = entry("a", "A");
    e.only_show_in = vec!["KDE".to_string(), "GNOME".to_string()];
    assert!(visible_in(&e, &["GNOME"]));
    assert!(!visible_in(&e, &["XFCE"]));
    assert!(!visible_in(&e, &[]));
    assert!(!visible_in(&e, &["kde"])); // case-sensitive
}

#[test]
fn visible_not_show_in_hides() {
    let mut e = entry("a", "A");
    e.not_show_in = vec!["KDE".to_string()];
    assert!(!visible_in(&e, &["KDE"]));
    assert!(visible_in(&e, &["GNOME"]));
}

#[test]
fn visible_both_lists() {
    let mut e = entry("a", "A");
    e.only_show_in = vec!["KDE".to_string()];
    e.not_show_in = vec!["GNOME".to_string()];
    assert!(visible_in(&e, &["KDE"]));
    assert!(!visible_in(&e, &["KDE", "GNOME"]));
}

#[test]
fn catalog_new_empty() {
    let c = Catalog::new();
    assert!(c.is_empty());
    assert_eq!(c.len(), 0);
    assert!(c.search("anything", &["KDE"]).is_empty());
}

#[test]
fn catalog_earlier_dir_wins_same_id() {
    let mut c = Catalog::new();
    c.add_dir_entries(1, vec![entry("demo.desktop", "High")]);
    c.add_dir_entries(5, vec![entry("demo.desktop", "Low")]);
    assert_eq!(c.len(), 1);
    assert_eq!(c.get("demo.desktop").unwrap().name, "High");
    // reversed registration order: rank 5 first, then rank 1 wins anyway
    let mut c2 = Catalog::new();
    c2.add_dir_entries(5, vec![entry("demo.desktop", "Low")]);
    c2.add_dir_entries(1, vec![entry("demo.desktop", "High")]);
    assert_eq!(c2.get("demo.desktop").unwrap().name, "High");
}

#[test]
fn catalog_later_dir_never_overrides() {
    let mut c = Catalog::new();
    c.add_dir_entries(1, vec![entry("a.desktop", "A")]);
    c.add_dir_entries(1, vec![entry("a.desktop", "B")]);
    assert_eq!(c.get("a.desktop").unwrap().name, "A");
}

#[test]
fn catalog_hidden_higher_priority_masks_lower() {
    let mut c = Catalog::new();
    c.add_dir_entries(5, vec![entry("demo.desktop", "VisibleLower")]);
    c.add_dir_entries(1, vec![hidden_entry("demo.desktop")]);
    // id still stored (as hidden) and hidden from search
    assert!(c.get("demo.desktop").is_some());
    assert!(c.search("demo", &["KDE"]).is_empty());
}

#[test]
fn catalog_lower_priority_hidden_does_not_mask_higher_visible() {
    let mut c = Catalog::new();
    c.add_dir_entries(1, vec![entry("demo.desktop", "VisibleHigh")]);
    c.add_dir_entries(5, vec![hidden_entry("demo.desktop")]);
    assert_eq!(c.get("demo.desktop").unwrap().name, "VisibleHigh");
    assert_eq!(c.search("demo", &["KDE"]).len(), 1);
}

#[test]
fn catalog_search_substring_case_insensitive() {
    let mut c = Catalog::new();
    c.add_dir_entries(0, vec![entry("one.desktop", "Firefox Beta")]);
    c.add_dir_entries(0, vec![entry("two.desktop", "firefox")]);
    let hits = c.search("FIREFOX", &["KDE"]);
    assert_eq!(hits.len(), 2);
}

#[test]
fn catalog_search_matches_keywords_and_desktop_id() {
    let mut c = Catalog::new();
    let mut e = entry("org.example.demo.desktop", "Demo");
    e.keywords = vec!["editor".to_string()];
    c.add_dir_entries(0, vec![e]);
    assert_eq!(c.search("editor", &["KDE"]).len(), 1);
    assert_eq!(c.search("org.example", &["KDE"]).len(), 1);
}

#[test]
fn catalog_search_ranking_prefix_substring_other() {
    let mut c = Catalog::new();
    let mut kw = entry("kw.desktop", "zzz");
    kw.keywords = vec!["firefox".to_string()];
    c.add_dir_entries(
        0,
        vec![
            entry("a.desktop", "firefox-stable"),
            entry("b.desktop", "xfirefox"),
            kw,
        ],
    );
    let hits = c.search("firefox", &["KDE"]);
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].desktop_id, "a.desktop"); // name prefix
    assert_eq!(hits[1].desktop_id, "b.desktop"); // name substring
    assert_eq!(hits[2].desktop_id, "kw.desktop"); // keyword match
}

#[test]
fn catalog_search_tie_by_name_then_id() {
    let mut c = Catalog::new();
    c.add_dir_entries(
        0,
        vec![
            entry("z.desktop", "Same Name"),
            entry("a.desktop", "Same Name"),
        ],
    );
    let hits = c.search("same", &["KDE"]);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].desktop_id, "a.desktop");
    assert_eq!(hits[1].desktop_id, "z.desktop");
}

#[test]
fn catalog_search_only_visible() {
    let mut c = Catalog::new();
    let mut no = entry("no.desktop", "HiddenApp");
    no.hidden = true;
    let mut nd = entry("nd.desktop", "NoDisplayApp");
    nd.no_display = true;
    let mut os = entry("os.desktop", "OtherSession");
    os.only_show_in = vec!["XFCE".to_string()];
    c.add_dir_entries(0, vec![entry("vis.desktop", "VisibleApp"), no, nd, os]);
    let hits = c.search("App", &["KDE"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].desktop_id, "vis.desktop");
}

#[test]
fn serde_camelcase_roundtrip() {
    let e = AppEntry {
        desktop_id: "demo.desktop".to_string(),
        name: "Demo".to_string(),
        generic_name: "Generic".to_string(),
        comment: "A comment".to_string(),
        icon: "demo-icon".to_string(),
        exec: "demo --flag %U".to_string(),
        try_exec: "which demo".to_string(),
        terminal: true,
        no_display: false,
        hidden: false,
        only_show_in: vec!["KDE".to_string()],
        not_show_in: vec![],
        categories: vec!["Utility".to_string()],
        keywords: vec!["a".to_string()],
    };
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"desktopId\""));
    assert!(json.contains("\"genericName\""));
    assert!(json.contains("\"tryExec\""));
    assert!(json.contains("\"noDisplay\""));
    assert!(json.contains("\"onlyShowIn\""));
    assert!(!json.contains("desktop_id"));
    assert!(!json.contains("snake_case"));
    let back: AppEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
    // Missing optional fields deserialize to defaults.
    let minimal = r#"{"desktopId":"x","name":"N","exec":"e"}"#;
    let m: AppEntry = serde_json::from_str(minimal).unwrap();
    assert_eq!(m.name, "N");
    assert!(m.categories.is_empty());
    assert!(!m.terminal);
}
