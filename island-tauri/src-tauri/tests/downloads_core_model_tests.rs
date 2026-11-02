use hermes_island_lib::integrations::downloads::core::{
    display_size, ConfigError, DirEvent, DownloadItem, DownloadsConfig, DownloadsModel, FileKind,
    FileStat, ItemState,
};

fn file(name: &str, size: u64, mtime: u64) -> FileStat {
    FileStat {
        name: name.to_string(),
        size_bytes: size,
        mtime_ms: mtime,
        is_dir: false,
        is_symlink: false,
    }
}

fn model(cfg: DownloadsConfig) -> DownloadsModel {
    match DownloadsModel::new(cfg) {
        Ok(m) => m,
        Err(e) => panic!("config should be valid: {e:?}"),
    }
}

fn names(items: &[DownloadItem]) -> Vec<String> {
    items.iter().map(|i| i.name.clone()).collect()
}

#[test]
fn config_validation_rejects_out_of_range() {
    assert!(matches!(
        DownloadsModel::new(DownloadsConfig {
            max_items: 0,
            ..Default::default()
        }),
        Err(ConfigError::MaxItemsOutOfRange(0))
    ));
    assert!(matches!(
        DownloadsModel::new(DownloadsConfig {
            max_items: 501,
            ..Default::default()
        }),
        Err(ConfigError::MaxItemsOutOfRange(501))
    ));
    assert!(matches!(
        DownloadsModel::new(DownloadsConfig {
            settle_ms: 60_001,
            ..Default::default()
        }),
        Err(ConfigError::SettleMsOutOfRange(60_001))
    ));
}

#[test]
fn config_validation_accepts_boundaries() {
    assert!(DownloadsModel::new(DownloadsConfig {
        max_items: 1,
        settle_ms: 0,
        show_hidden: true
    })
    .is_ok());
    assert!(DownloadsModel::new(DownloadsConfig {
        max_items: 500,
        settle_ms: 60_000,
        show_hidden: false
    })
    .is_ok());
    assert!(DownloadsModel::new(DownloadsConfig::default()).is_ok());
}

#[test]
fn hidden_names_filtered_unless_show_hidden() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file(".secret", 10, 100)));
    m.apply(0, DirEvent::Upsert(file("visible", 10, 100)));
    assert_eq!(names(&m.items(0)), vec!["visible"]);
    assert_eq!(m.rejected_count(), 0, "hidden names are not rejected");

    let mut m2 = model(DownloadsConfig {
        show_hidden: true,
        ..Default::default()
    });
    m2.apply(0, DirEvent::Upsert(file(".secret", 10, 100)));
    assert_eq!(names(&m2.items(0)), vec![".secret"]);
}

#[test]
fn hidden_pdf_respects_show_hidden() {
    // show_hidden: true -> .hidden.pdf appears in items().
    let mut show = model(DownloadsConfig {
        show_hidden: true,
        ..Default::default()
    });
    show.apply(0, DirEvent::Upsert(file(".hidden.pdf", 10, 100)));
    assert_eq!(names(&show.items(0)), vec![".hidden.pdf"]);
    assert_eq!(show.rejected_count(), 0, "hidden names are not rejected");

    // show_hidden: false -> .hidden.pdf is filtered, still not rejected.
    let mut hide = model(DownloadsConfig::default());
    hide.apply(0, DirEvent::Upsert(file(".hidden.pdf", 10, 100)));
    assert!(hide.items(0).is_empty());
    assert_eq!(hide.rejected_count(), 0, "hidden names are not rejected");
}

#[test]
fn rejected_names_are_counted_and_ignored() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("", 1, 1)));
    m.apply(0, DirEvent::Upsert(file(".", 1, 1)));
    m.apply(0, DirEvent::Upsert(file("..", 1, 1)));
    m.apply(0, DirEvent::Upsert(file("a/b", 1, 1)));
    m.apply(0, DirEvent::Upsert(file("a\0b", 1, 1)));
    assert_eq!(m.rejected_count(), 5);
    assert!(m.items(0).is_empty());
}

#[test]
fn dirs_and_symlinks_ignored() {
    let mut m = model(DownloadsConfig::default());
    m.apply(
        0,
        DirEvent::Upsert(FileStat {
            name: "d".into(),
            size_bytes: 0,
            mtime_ms: 0,
            is_dir: true,
            is_symlink: false,
        }),
    );
    m.apply(
        0,
        DirEvent::Upsert(FileStat {
            name: "l".into(),
            size_bytes: 0,
            mtime_ms: 0,
            is_dir: false,
            is_symlink: true,
        }),
    );
    assert!(m.items(0).is_empty());
    assert_eq!(m.rejected_count(), 0);
}

#[test]
fn settling_to_ready_at_exact_deadline() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    assert_eq!(m.next_settle_at(), Some(1000));
    assert_eq!(m.items(999)[0].state, ItemState::Settling);
    assert_eq!(m.items(1000)[0].state, ItemState::Ready); // expiry inclusive
    m.tick(1000);
    assert_eq!(m.next_settle_at(), None);
    assert_eq!(m.items(1000)[0].state, ItemState::Ready);
}

#[test]
fn size_change_mid_settle_restarts_window() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.apply(500, DirEvent::Upsert(file("a", 20, 100))); // size changed -> deadline 1500
    assert_eq!(m.items(1000)[0].state, ItemState::Settling);
    assert_eq!(m.items(1499)[0].state, ItemState::Settling);
    assert_eq!(m.items(1500)[0].state, ItemState::Ready);
}

#[test]
fn mtime_change_mid_settle_restarts_window() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.apply(500, DirEvent::Upsert(file("a", 10, 200))); // mtime changed -> deadline 1500
    assert_eq!(m.items(1000)[0].state, ItemState::Settling);
    assert_eq!(m.items(1500)[0].state, ItemState::Ready);
}

#[test]
fn identical_stat_upsert_keeps_window() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.apply(900, DirEvent::Upsert(file("a", 10, 100))); // same stat -> no restart
    assert_eq!(m.items(999)[0].state, ItemState::Settling);
    assert_eq!(m.items(1000)[0].state, ItemState::Ready);
}

#[test]
fn upsert_replaces_stat_in_place() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.apply(0, DirEvent::Upsert(file("a", 999, 100)));
    let items = m.items(0);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].size_bytes, 999);
    assert_eq!(items[0].state, ItemState::Settling);
}

#[test]
fn partial_suffixes_case_insensitive() {
    let cases: &[(&str, bool)] = &[
        ("a.part", true),
        ("a.PART", true),
        ("a.Part", true),
        ("a.crdownload", true),
        ("a.CRDOWNLOAD", true),
        ("a.download", true),
        ("a.partial", true),
        ("a.pdf.partial", true),
        ("a.opdownload", true),
        ("a.txt", false),
        ("apart", false),
        ("a.parta", false),
        ("a.partialx", false),
    ];
    for &(name, expect_partial) in cases {
        let mut m = model(DownloadsConfig::default());
        m.apply(0, DirEvent::Upsert(file(name, 10, 100)));
        let items = m.items(0);
        assert_eq!(items.len(), 1, "name {name}");
        if expect_partial {
            assert_eq!(items[0].state, ItemState::InProgress, "name {name}");
        } else {
            assert_eq!(items[0].state, ItemState::Settling, "name {name}");
        }
    }
}

#[test]
fn in_progress_items_never_become_ready() {
    let mut m = model(DownloadsConfig {
        settle_ms: 10,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a.part", 5, 0)));
    m.tick(1000);
    assert_eq!(m.items(1000)[0].state, ItemState::InProgress);
    assert_eq!(m.next_settle_at(), None);
}

#[test]
fn rename_in_progress_to_final_yields_single_settling_item() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("big.zip.part", 500, 10)));
    assert_eq!(m.items(0)[0].state, ItemState::InProgress);
    m.apply(
        100,
        DirEvent::Renamed {
            from: "big.zip.part".into(),
            to: file("big.zip", 500, 10),
        },
    );
    let items = m.items(200);
    assert_eq!(items.len(), 1, "one entry, not two");
    assert_eq!(items[0].name, "big.zip");
    assert_eq!(items[0].size_bytes, 500);
    assert_eq!(items[0].state, ItemState::Settling);
    assert_eq!(m.items(1100)[0].state, ItemState::Ready); // deadline 100 + 1000
}

#[test]
fn rename_final_to_partial_marks_in_progress() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("report.pdf", 10, 100)));
    m.apply(
        0,
        DirEvent::Renamed {
            from: "report.pdf".into(),
            to: file("report.pdf.part", 10, 100),
        },
    );
    let items = m.items(0);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "report.pdf.part");
    assert_eq!(items[0].state, ItemState::InProgress);
}

#[test]
fn rename_unknown_source_is_fine() {
    let mut m = model(DownloadsConfig::default());
    m.apply(
        0,
        DirEvent::Renamed {
            from: "ghost.part".into(),
            to: file("ghost", 1, 1),
        },
    );
    let items = m.items(0);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "ghost");
    assert_eq!(items[0].state, ItemState::Settling);
}

#[test]
fn removed_unknown_is_noop() {
    let mut m = model(DownloadsConfig::default());
    m.apply(
        0,
        DirEvent::Removed {
            name: "ghost".into(),
        },
    );
    assert!(m.items(0).is_empty());
    assert_eq!(m.rejected_count(), 0);
}

#[test]
fn removed_known_removes_item() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("a", 1, 1)));
    m.apply(0, DirEvent::Removed { name: "a".into() });
    assert!(m.items(0).is_empty());
}

#[test]
fn max_items_evicts_oldest() {
    let mut m = model(DownloadsConfig {
        max_items: 3,
        settle_ms: 0,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 1, 100)));
    m.apply(0, DirEvent::Upsert(file("b", 1, 200)));
    m.apply(0, DirEvent::Upsert(file("c", 1, 300)));
    m.apply(0, DirEvent::Upsert(file("d", 1, 400)));
    m.apply(0, DirEvent::Upsert(file("e", 1, 500)));
    assert_eq!(names(&m.items(0)), vec!["e", "d", "c"]);
}

#[test]
fn evicted_file_returns_with_newer_mtime() {
    let mut m = model(DownloadsConfig {
        max_items: 2,
        settle_ms: 0,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 1, 10)));
    m.apply(0, DirEvent::Upsert(file("b", 1, 20)));
    m.apply(0, DirEvent::Upsert(file("c", 1, 30))); // a evicted
    assert_eq!(m.items(0).len(), 2);
    m.apply(0, DirEvent::Upsert(file("a", 1, 40))); // newer mtime -> back
    assert_eq!(names(&m.items(0)), vec!["a", "c"]);
}

#[test]
fn eviction_ties_break_by_name_ascending() {
    let mut m = model(DownloadsConfig {
        max_items: 2,
        settle_ms: 0,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("z", 1, 50)));
    m.apply(0, DirEvent::Upsert(file("a", 1, 50)));
    m.apply(0, DirEvent::Upsert(file("m", 1, 50)));
    assert_eq!(names(&m.items(0)), vec!["a", "m"]);
}

#[test]
fn equal_mtime_ties_break_by_name_ascending() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("z", 1, 50)));
    m.apply(0, DirEvent::Upsert(file("a", 1, 50)));
    m.apply(0, DirEvent::Upsert(file("m", 1, 50)));
    assert_eq!(names(&m.items(0)), vec!["a", "m", "z"]);
}

#[test]
fn byte_order_name_ties() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("B", 1, 7)));
    m.apply(0, DirEvent::Upsert(file("a", 1, 7)));
    // ASCII 'B' (0x42) sorts before 'a' (0x61)
    assert_eq!(names(&m.items(0)), vec!["B", "a"]);
}

#[test]
fn items_sorted_by_mtime_desc() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("old", 1, 10)));
    m.apply(0, DirEvent::Upsert(file("mid", 1, 20)));
    m.apply(0, DirEvent::Upsert(file("new", 1, 30)));
    assert_eq!(names(&m.items(0)), vec!["new", "mid", "old"]);
}

#[test]
fn kinds_flow_through_items() {
    let cases: &[(&str, FileKind)] = &[
        ("a.zip", FileKind::Archive),
        ("a.tar.gz", FileKind::Archive),
        ("a.png", FileKind::Image),
        ("a.mp4", FileKind::Video),
        ("a.flac", FileKind::Audio),
        ("a.pdf", FileKind::Document),
        ("a.pkg.tar.zst", FileKind::Package),
        ("a.APPIMAGE", FileKind::Package),
        ("a.rs", FileKind::Code),
        (".gitignore", FileKind::Other),
        ("README", FileKind::Other),
    ];
    for &(name, kind) in cases {
        let mut m = model(DownloadsConfig {
            show_hidden: true,
            ..Default::default()
        });
        m.apply(0, DirEvent::Upsert(file(name, 1, 1)));
        let items = m.items(0);
        assert_eq!(items.len(), 1, "name {name}");
        assert_eq!(items[0].kind, kind, "name {name}");
    }
}

#[test]
fn leading_dot_only_names_classify_as_other() {
    // A name whose only dot is the leading hidden dot (empty stem) is Other,
    // even when the trailing part looks like a known extension. ".zip" /
    // ".pdf" must NOT classify as Archive / Document.
    for name in [".zip", ".pdf", "README"] {
        let mut m = model(DownloadsConfig {
            show_hidden: true,
            ..Default::default()
        });
        m.apply(0, DirEvent::Upsert(file(name, 1, 1)));
        let items = m.items(0);
        assert_eq!(items.len(), 1, "name {name}");
        assert_eq!(items[0].kind, FileKind::Other, "name {name}");
    }
}

#[test]
fn leading_dot_stem_classifies_by_extension() {
    // ".config.zip" has a non-empty stem (".config"), so it classifies
    // normally as an Archive despite its leading dot.
    let mut m = model(DownloadsConfig {
        show_hidden: true,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file(".config.zip", 1, 1)));
    assert_eq!(m.items(0)[0].kind, FileKind::Archive);
}

#[test]
fn display_size_boundaries() {
    assert_eq!(display_size(0), "0 B");
    assert_eq!(display_size(1023), "1023 B");
    assert_eq!(display_size(1024), "1.0 KB");
    assert_eq!(display_size(1536), "1.5 KB");
    assert_eq!(display_size(1048575), "1023.9 KB");
    assert_eq!(display_size(1048576), "1.0 MB");
    assert_eq!(display_size(1073741824), "1.0 GB");
    assert_eq!(display_size(1099511627776), "1.0 TB");
}

#[test]
fn display_size_u64_max_does_not_panic() {
    let s = display_size(u64::MAX);
    assert!(s.ends_with(" TB"));
}

#[test]
fn rescan_preserves_ready_for_unchanged() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.tick(2000);
    m.apply(5000, DirEvent::Rescan(vec![file("a", 10, 100)]));
    assert_eq!(m.items(5000)[0].state, ItemState::Ready);
}

#[test]
fn rescan_preserves_settling_deadline_when_unchanged() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100))); // deadline 1000
    m.apply(500, DirEvent::Rescan(vec![file("a", 10, 100)]));
    assert_eq!(m.items(999)[0].state, ItemState::Settling);
    assert_eq!(m.items(1000)[0].state, ItemState::Ready); // original deadline preserved
}

#[test]
fn rescan_restarts_settling_for_changed() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 10, 100)));
    m.tick(2000);
    m.apply(5000, DirEvent::Rescan(vec![file("a", 99, 100)])); // size changed
    assert_eq!(m.items(5000)[0].state, ItemState::Settling);
    assert_eq!(m.items(6000)[0].state, ItemState::Ready);
}

#[test]
fn rescan_drops_items_not_present() {
    let mut m = model(DownloadsConfig::default());
    m.apply(0, DirEvent::Upsert(file("a", 1, 1)));
    m.apply(0, DirEvent::Upsert(file("b", 1, 2)));
    m.apply(0, DirEvent::Rescan(vec![file("a", 1, 1)]));
    assert_eq!(names(&m.items(0)), vec!["a"]);
}

#[test]
fn rescan_counts_rejected_names() {
    let mut m = model(DownloadsConfig::default());
    m.apply(
        0,
        DirEvent::Rescan(vec![file("bad/name", 1, 1), file("ok", 1, 2)]),
    );
    assert_eq!(m.rejected_count(), 1);
    assert_eq!(names(&m.items(0)), vec!["ok"]);
}

#[test]
fn next_settle_at_is_earliest_deadline() {
    let mut m = model(DownloadsConfig {
        settle_ms: 1000,
        ..Default::default()
    });
    m.apply(0, DirEvent::Upsert(file("a", 1, 1))); // deadline 1000
    m.apply(500, DirEvent::Upsert(file("b", 1, 2))); // deadline 1500
    assert_eq!(m.next_settle_at(), Some(1000));
    m.tick(1000);
    assert_eq!(m.next_settle_at(), Some(1500));
    m.tick(1500);
    assert_eq!(m.next_settle_at(), None);
}

#[test]
fn settle_ms_zero_marks_ready_immediately() {
    let mut m = model(DownloadsConfig {
        settle_ms: 0,
        ..Default::default()
    });
    m.apply(5, DirEvent::Upsert(file("a", 1, 1)));
    assert_eq!(m.items(5)[0].state, ItemState::Ready);
    m.tick(5);
    assert_eq!(m.next_settle_at(), None);
}

#[test]
fn download_item_serde_camelcase_round_trip() {
    let item = DownloadItem {
        name: "photo.png".into(),
        size_bytes: 2048,
        mtime_ms: 123456,
        kind: FileKind::Image,
        state: ItemState::Ready,
    };
    let json = serde_json::to_string(&item).unwrap();
    assert_eq!(
        json,
        r#"{"name":"photo.png","sizeBytes":2048,"mtimeMs":123456,"kind":"image","state":"ready"}"#
    );
    let back: DownloadItem = serde_json::from_str(&json).unwrap();
    assert_eq!(back, item);
}

#[test]
fn file_stat_serde_camelcase() {
    let st = FileStat {
        name: "a".into(),
        size_bytes: 1,
        mtime_ms: 2,
        is_dir: false,
        is_symlink: true,
    };
    let json = serde_json::to_string(&st).unwrap();
    assert_eq!(
        json,
        r#"{"name":"a","sizeBytes":1,"mtimeMs":2,"isDir":false,"isSymlink":true}"#
    );
    let back: FileStat = serde_json::from_str(&json).unwrap();
    assert_eq!(back, st);
}

#[test]
fn dir_event_serde_camelcase_round_trip() {
    let ev = DirEvent::Renamed {
        from: "a.part".into(),
        to: file("a", 10, 20),
    };
    let json = serde_json::to_string(&ev).unwrap();
    let back: DirEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ev);
}
