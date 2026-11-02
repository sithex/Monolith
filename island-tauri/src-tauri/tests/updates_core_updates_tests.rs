use hermes_island_lib::integrations::updates::core::{
    backoff_minutes, is_kernel, parse_checkupdates, parse_paru_qua, Badge, Schedule, Source,
    Update, UpdatesError, CHECKUPDATES_ARGV, MAX_STDOUT_BYTES, PARU_QUA_ARGV,
};

fn upd(name: &str, source: Source, ignored: bool) -> Update {
    Update {
        name: name.to_string(),
        old_version: "1-1".to_string(),
        new_version: "2-1".to_string(),
        ignored,
        source,
    }
}

fn cmd_failed() -> Result<Vec<Update>, UpdatesError> {
    Err(UpdatesError::CommandFailed)
}

// ---------- checkupdates parsing ----------

#[test]
fn checkupdates_exit0_parses() {
    let out =
        parse_checkupdates(0, "linux 6.1-1 -> 6.2-1\nfirefox 120.0-1 -> 121.0-1\n", "").unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].name, "linux");
    assert_eq!(out[0].old_version, "6.1-1");
    assert_eq!(out[0].new_version, "6.2-1");
    assert!(!out[0].ignored);
    assert_eq!(out[0].source, Source::Repo);
    assert_eq!(out[1].name, "firefox");
}

#[test]
fn checkupdates_exit2_no_updates() {
    let out = parse_checkupdates(2, "", "").unwrap();
    assert!(out.is_empty());
    // stdout is ignored on exit 2 (none by definition)
    let out = parse_checkupdates(2, "linux 6.1-1 -> 6.2-1\n", "").unwrap();
    assert!(out.is_empty());
}

#[test]
fn checkupdates_exit1_is_failure() {
    assert_eq!(
        parse_checkupdates(1, "", ""),
        Err(UpdatesError::CommandFailed)
    );
    assert_eq!(
        parse_checkupdates(3, "", ""),
        Err(UpdatesError::CommandFailed)
    );
    assert_eq!(
        parse_checkupdates(-1, "", ""),
        Err(UpdatesError::CommandFailed)
    );
}

#[test]
fn checkupdates_stderr_ignored() {
    // stderr is not consulted for checkupdates; exit 0 wins
    let out = parse_checkupdates(0, "linux 6.1-1 -> 6.2-1\n", "warning: junk").unwrap();
    assert_eq!(out.len(), 1);
}

#[test]
fn checkupdates_lines_crlf() {
    let out = parse_checkupdates(0, "linux 6.1-1 -> 6.2-1\r\n", "").unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "linux");
}

// ---------- paru parsing ----------

#[test]
fn paru_exit0_parses() {
    let out = parse_paru_qua(0, "paru-git 1.0-1 -> 2.0-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].source, Source::Aur);
}

#[test]
fn paru_exit1_empty_is_none() {
    let out = parse_paru_qua(1, "", "").unwrap();
    assert!(out.is_empty());
}

#[test]
fn paru_exit1_stderr_is_failure() {
    assert_eq!(
        parse_paru_qua(1, "", "error: no results"),
        Err(UpdatesError::CommandFailed)
    );
    assert_eq!(
        parse_paru_qua(1, "linux 6.1-1 -> 6.2-1\n", "error: no results"),
        Err(UpdatesError::CommandFailed)
    );
}

#[test]
fn paru_exit1_stdout_only_parses() {
    let out = parse_paru_qua(1, "paru-git 1.0-1 -> 2.0-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
}

#[test]
fn paru_other_exit_is_failure() {
    assert_eq!(parse_paru_qua(2, "", ""), Err(UpdatesError::CommandFailed));
    assert_eq!(
        parse_paru_qua(127, "", ""),
        Err(UpdatesError::CommandFailed)
    );
}

// ---------- line-level parsing ----------

#[test]
fn malformed_line_skipped_others_kept() {
    let out = parse_checkupdates(
        0,
        "linux 6.1-1 -> 6.2-1\nGARBAGE LINE\nfirefox 120.0-1 -> 121.0-1\n",
        "",
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].name, "linux");
    assert_eq!(out[1].name, "firefox");
}

#[test]
fn all_malformed_is_error() {
    assert_eq!(
        parse_checkupdates(0, "GARBAGE LINE\nalso bad\n", ""),
        Err(UpdatesError::AllMalformed)
    );
    assert_eq!(
        parse_paru_qua(0, "GARBAGE LINE\n", ""),
        Err(UpdatesError::AllMalformed)
    );
}

#[test]
fn blank_lines_skipped() {
    let out = parse_checkupdates(0, "\n\nlinux 6.1-1 -> 6.2-1\n\n", "").unwrap();
    assert_eq!(out.len(), 1);
    // trailing newline produces an empty final segment; not malformed
    let out = parse_checkupdates(0, "linux 6.1-1 -> 6.2-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
}

#[test]
fn ignored_marker_parsed() {
    let out = parse_checkupdates(0, "linux 6.1-1 -> 6.2-1 [ignored]\n", "").unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0].ignored);
    assert_eq!(out[0].name, "linux");
}

#[test]
fn ignored_marker_must_be_exact() {
    // a bad marker is skipped; a good line survives
    let out =
        parse_checkupdates(0, "linux 6.1-1 -> 6.2-1 [ignoredx]\nff 1-1 -> 2-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "ff");
    // a lone malformed line is AllMalformed, not empty
    assert_eq!(
        parse_checkupdates(0, "linux 6.1-1 -> 6.2-1 [ignored] extra\n", ""),
        Err(UpdatesError::AllMalformed)
    );
}

#[test]
fn name_validation_edges() {
    const VALID: &str = "firefox 1-1 -> 2-1\n";
    // leading '-' / '.' are rejected; the valid line survives
    let out = parse_checkupdates(
        0,
        "-linux 6.1-1 -> 6.2-1\n.linux 6.1-1 -> 6.2-1\nfirefox 1-1 -> 2-1\n",
        "",
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "firefox");
    // uppercase rejected
    let out = parse_checkupdates(0, &format!("Firefox 1-1 -> 2-1\n{VALID}"), "").unwrap();
    assert_eq!(out.len(), 1);
    // >128 bytes rejected
    let long = "a".repeat(129);
    let out = parse_checkupdates(0, &format!("{long} 1-1 -> 2-1\n{VALID}"), "").unwrap();
    assert_eq!(out.len(), 1);
    // allowed characters all pass
    let out = parse_checkupdates(0, "a_b.c+d-e@f 1-1 -> 2-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
    // all-malformed input is an error, not a fabricated empty list
    assert_eq!(
        parse_checkupdates(0, "-linux 6.1-1 -> 6.2-1\n", ""),
        Err(UpdatesError::AllMalformed)
    );
}

#[test]
fn version_validation_edges() {
    const VALID: &str = "firefox 1-1 -> 2-1\n";
    // epoch colon + pkgrel fine
    let out = parse_checkupdates(0, "linux 1:6.1-1 -> 1:6.2-1\n", "").unwrap();
    assert_eq!(out.len(), 1);
    // '=' not allowed (skipped; valid line survives)
    let out = parse_checkupdates(0, &format!("linux 6.1=1 -> 6.2-1\n{VALID}"), "").unwrap();
    assert_eq!(out.len(), 1);
    // >128 bytes rejected
    let long = "a".repeat(129);
    let out = parse_checkupdates(0, &format!("linux {long} -> 6.2-1\n{VALID}"), "").unwrap();
    assert_eq!(out.len(), 1);
    // whitespace inside a version splits tokens -> malformed
    let out = parse_checkupdates(0, &format!("linux 6.1 1 -> 6.2-1\n{VALID}"), "").unwrap();
    assert_eq!(out.len(), 1);
    // all-malformed input is an error, not empty
    assert_eq!(
        parse_checkupdates(0, "linux 6.1=1 -> 6.2-1\n", ""),
        Err(UpdatesError::AllMalformed)
    );
}

// ---------- caps ----------

#[test]
fn stdout_cap_strict_boundary() {
    // exactly 1 MiB of a single malformed line: too-large must NOT fire
    let exactly = "a".repeat(MAX_STDOUT_BYTES);
    assert_eq!(
        parse_checkupdates(0, &exactly, ""),
        Err(UpdatesError::AllMalformed)
    );
    let over = "a".repeat(MAX_STDOUT_BYTES + 1);
    assert_eq!(
        parse_checkupdates(0, &over, ""),
        Err(UpdatesError::StdoutTooLarge)
    );
}

#[test]
fn lines_cap_exact_boundary() {
    // Literal 10 000, independent of MAX_LINES, so lowering the constant
    // is caught (a test deriving its expectation from the constant would
    // be blind to it).
    const TEN_THOUSAND: usize = 10_000;
    let mut s = String::new();
    for i in 0..TEN_THOUSAND {
        s.push_str(&format!("pkg{i} 1-1 -> 2-1\n"));
    }
    let out = parse_checkupdates(0, &s, "").unwrap();
    assert_eq!(out.len(), TEN_THOUSAND);

    s.push_str("pkg-extra 1-1 -> 2-1\n");
    assert_eq!(
        parse_checkupdates(0, &s, ""),
        Err(UpdatesError::TooManyLines)
    );
}

// ---------- is_kernel ----------

#[test]
fn is_kernel_linux_exact() {
    assert!(is_kernel("linux"));
}

#[test]
fn is_kernel_linux_cachyos() {
    assert!(is_kernel("linux-cachyos"));
}

#[test]
fn is_kernel_linux_headers() {
    assert!(is_kernel("linux-cachyos-headers"));
}

#[test]
fn is_kernel_nvidia_prefix() {
    assert!(is_kernel("nvidia"));
    assert!(is_kernel("nvidia-dkms"));
    assert!(is_kernel("nvidia-utils"));
}

#[test]
fn is_kernel_mesa() {
    assert!(is_kernel("mesa"));
}

#[test]
fn is_kernel_false_for_unrelated() {
    assert!(!is_kernel("firefox"));
    assert!(!is_kernel("python"));
    assert!(!is_kernel("systemd"));
    assert!(!is_kernel("linuxqq"));
    assert!(!is_kernel(""));
}

// ---------- Badge::compute ----------

#[test]
fn badge_counts_all_ok() {
    let repo = Ok(vec![
        upd("a", Source::Repo, false),
        upd("b", Source::Repo, false),
    ]);
    let aur = Some(Ok(vec![upd("c", Source::Aur, false)]));
    let b = Badge::compute(repo, aur, 1_000_000, Some(999_000), None);
    assert_eq!(b.count, Some(3));
    assert_eq!(b.repo_count, Some(2));
    assert_eq!(b.aur_count, Some(1));
    assert_eq!(b.error, None);
    assert_eq!(b.items.len(), 3);
}

#[test]
fn badge_count_excludes_ignored() {
    let repo = Ok(vec![
        upd("a", Source::Repo, false),
        upd("ignored-pkg", Source::Repo, true),
    ]);
    let b = Badge::compute(repo, None, 1_000_000, Some(999_000), None);
    assert_eq!(b.count, Some(1));
    assert_eq!(b.repo_count, Some(1));
    // ignored item still present in items, annotated
    assert_eq!(b.items.len(), 2);
    assert!(b.items.iter().any(|u| u.ignored && u.name == "ignored-pkg"));
}

#[test]
fn badge_repo_aur_sum_invariant() {
    let repo = Ok(vec![
        upd("a", Source::Repo, false),
        upd("b", Source::Repo, true),
    ]);
    let aur = Some(Ok(vec![
        upd("c", Source::Aur, false),
        upd("d", Source::Aur, false),
        upd("e", Source::Aur, true),
    ]));
    let b = Badge::compute(repo, aur, 1_000_000, Some(999_000), None);
    assert_eq!(
        b.repo_count.unwrap() + b.aur_count.unwrap(),
        b.count.unwrap()
    );
    assert_eq!(b.count, Some(3));
    assert_eq!(b.repo_count, Some(1));
    assert_eq!(b.aur_count, Some(2));
}

#[test]
fn badge_repo_failure_unknown_not_zero() {
    let b = Badge::compute(cmd_failed(), None, 1_000_000, Some(999_000), None);
    assert_eq!(b.count, None);
    assert_eq!(b.repo_count, None);
    assert_eq!(b.aur_count, None);
    assert!(b.error.is_some());
    assert!(b.items.is_empty());
}

#[test]
fn badge_repo_failure_with_prev_fallback() {
    let prev = vec![upd("a", Source::Repo, false)];
    let b = Badge::compute(cmd_failed(), None, 1_000_000, Some(999_000), Some(&prev));
    // counts fall back to the last OK result; error still records the failure
    assert_eq!(b.count, Some(1));
    assert_eq!(b.repo_count, Some(1));
    assert!(b.error.is_some());
    assert_eq!(b.items.len(), 1);
    assert_eq!(b.items[0].name, "a");
}

#[test]
fn badge_repo_failure_prev_zero_is_known_zero() {
    let prev: Vec<Update> = Vec::new();
    let b = Badge::compute(cmd_failed(), None, 1_000_000, Some(999_000), Some(&prev));
    // a previous OK result that was empty is a known zero, not unknown
    assert_eq!(b.count, Some(0));
    assert!(b.error.is_some());
}

#[test]
fn badge_aur_failure_unknown() {
    let repo = Ok(vec![upd("a", Source::Repo, false)]);
    let aur = Some(Err(UpdatesError::CommandFailed));
    let b = Badge::compute(repo, aur, 1_000_000, Some(999_000), None);
    assert_eq!(b.count, None);
    assert_eq!(b.aur_count, None);
    // known repo items are still shown
    assert_eq!(b.items.len(), 1);
    assert!(b.error.as_deref().unwrap_or("").contains("aur"));
}

#[test]
fn badge_aur_absent_is_known_empty() {
    let repo = Ok(vec![upd("a", Source::Repo, false)]);
    let b = Badge::compute(repo, None, 1_000_000, Some(999_000), None);
    assert_eq!(b.count, Some(1));
    assert_eq!(b.aur_count, Some(0));
}

#[test]
fn badge_stale_true_when_overdue() {
    // last OK exactly 6 h ago is NOT stale; 6 h + 1 s is
    let now = 2_000_000u64;
    let ok = now - 6 * 3600;
    let b = Badge::compute(Ok(Vec::new()), None, now, Some(ok), None);
    assert!(!b.stale);
    let b = Badge::compute(Ok(Vec::new()), None, now, Some(ok - 1), None);
    assert!(b.stale);
}

#[test]
fn badge_stale_true_never_ok() {
    let b = Badge::compute(Ok(Vec::new()), None, 1_000_000, None, None);
    assert!(b.stale);
}

#[test]
fn badge_needs_reboot_true_for_kernel() {
    let repo = Ok(vec![upd("linux-cachyos", Source::Repo, false)]);
    let b = Badge::compute(repo, None, 1_000_000, Some(999_000), None);
    assert!(b.needs_reboot);
}

#[test]
fn badge_needs_reboot_false_no_kernel() {
    let repo = Ok(vec![upd("firefox", Source::Repo, false)]);
    let b = Badge::compute(repo, None, 1_000_000, Some(999_000), None);
    assert!(!b.needs_reboot);
}

#[test]
fn badge_items_sorted_reboot_then_name() {
    let repo = Ok(vec![
        upd("zlib", Source::Repo, false),
        upd("nvidia", Source::Repo, false),
        upd("aaa", Source::Repo, false),
        upd("linux", Source::Repo, false),
    ]);
    let b = Badge::compute(repo, None, 1_000_000, Some(999_000), None);
    let names: Vec<&str> = b.items.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, vec!["linux", "nvidia", "aaa", "zlib"]);
}

#[test]
fn badge_serde_camel_case() {
    let b = Badge::compute(
        Ok(vec![upd("linux", Source::Repo, false)]),
        Some(Ok(vec![upd("paru-git", Source::Aur, true)])),
        1_000_000,
        Some(999_000),
        None,
    );
    let v = serde_json::to_value(&b).unwrap();
    let obj = v.as_object().unwrap();
    for key in [
        "count",
        "repoCount",
        "aurCount",
        "needsReboot",
        "stale",
        "error",
        "items",
    ] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
    let items = obj["items"].as_array().unwrap();
    let first = items[0].as_object().unwrap();
    for key in ["name", "oldVersion", "newVersion", "ignored", "source"] {
        assert!(first.contains_key(key), "missing item key {key}");
    }
    assert_eq!(first["source"], serde_json::json!("repo"));
    assert_eq!(items[1]["source"], serde_json::json!("aur"));
    assert_eq!(items[1]["ignored"], serde_json::json!(true));
    assert_eq!(obj["repoCount"], serde_json::json!(1));
    assert_eq!(obj["aurCount"], serde_json::json!(0));
}

#[test]
fn badge_serde_roundtrip() {
    let b = Badge::compute(
        Ok(vec![upd("linux-cachyos", Source::Repo, false)]),
        None,
        1_000_000,
        Some(999_000),
        None,
    );
    let js = serde_json::to_string(&b).unwrap();
    let back: Badge = serde_json::from_str(&js).unwrap();
    assert_eq!(back, b);
}

// ---------- Schedule ----------

#[test]
fn backoff_sequence() {
    assert_eq!(backoff_minutes(0), 10);
    assert_eq!(backoff_minutes(1), 10);
    assert_eq!(backoff_minutes(2), 20);
    assert_eq!(backoff_minutes(3), 40);
    assert_eq!(backoff_minutes(4), 60);
    assert_eq!(backoff_minutes(5), 60);
    assert_eq!(backoff_minutes(99), 60);
}

#[test]
fn due_never_attempted() {
    let s = Schedule::new(false);
    assert!(s.due(1_000_000, None, Some(999_000)));
}

#[test]
fn due_hourly_after_success() {
    let s = Schedule::new(false);
    let now = 1_000_000u64;
    let attempt = now - 59 * 60;
    let ok = attempt; // last attempt succeeded
    assert!(!s.due(now, Some(attempt), Some(ok)));
    let attempt = now - 61 * 60;
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_not_due_9min_after_first_failure() {
    let s = Schedule {
        on_battery: false,
        consecutive_failures: 1,
    };
    let now = 1_000_000u64;
    let attempt = now - 9 * 60;
    let ok = attempt - 3_600; // last success before the failed attempt
    assert!(!s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_due_11min_after_first_failure() {
    let s = Schedule {
        on_battery: false,
        consecutive_failures: 1,
    };
    let now = 1_000_000u64;
    let attempt = now - 11 * 60;
    let ok = attempt - 3_600;
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_backoff_grows() {
    let s = Schedule {
        on_battery: false,
        consecutive_failures: 3,
    };
    let now = 1_000_000u64;
    let attempt = now - 39 * 60;
    let ok = attempt - 3_600;
    assert!(!s.due(now, Some(attempt), Some(ok)));
    let attempt = now - 41 * 60;
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_backoff_capped_at_60() {
    let s = Schedule {
        on_battery: false,
        consecutive_failures: 4,
    };
    let now = 1_000_000u64;
    let attempt = now - 59 * 60;
    let ok = attempt - 3_600;
    assert!(!s.due(now, Some(attempt), Some(ok)));
    let attempt = now - 61 * 60;
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_success_resets_backoff() {
    let mut s = Schedule::new(false);
    s.note_failure();
    s.note_failure();
    s.note_failure();
    assert_eq!(s.consecutive_failures, 3);
    s.note_success();
    assert_eq!(s.consecutive_failures, 0);
    // after success, hourly cadence applies, not backoff
    let now = 1_000_000u64;
    let attempt = now - 30 * 60;
    assert!(!s.due(now, Some(attempt), Some(attempt)));
}

#[test]
fn due_battery_blocks_when_not_overdue() {
    let s = Schedule::new(true);
    let now = 1_000_000u64;
    let attempt = now - 2 * 3600; // would be due by cadence
    let ok = now - 60; // fresh success
    assert!(!s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_battery_allows_when_overdue() {
    let s = Schedule::new(true);
    let now = 1_000_000u64;
    let attempt = now - 7 * 3600;
    let ok = now - 7 * 3600 - 60; // last OK > 6 h ago
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_battery_overdue_still_backoff_capped() {
    let s = Schedule {
        on_battery: true,
        consecutive_failures: 4,
    };
    let now = 1_000_000u64;
    let attempt = now - 30 * 60; // failed attempt 30 min ago
    let ok = now - 7 * 3600; // stale (> 6 h)
                             // battery override lifts the block, but the 60-min backoff still applies
    assert!(!s.due(now, Some(attempt), Some(ok)));
    let attempt = now - 61 * 60;
    assert!(s.due(now, Some(attempt), Some(ok)));
}

#[test]
fn due_never_ok_on_battery_allows() {
    let s = Schedule::new(true);
    // attempt 2000 s ago: backoff (10 min) elapsed, battery block lifted by "never OK"
    assert!(s.due(1_000_000, Some(998_000), None));
}

#[test]
fn due_never_attempted_on_battery_not_overdue_blocks() {
    let s = Schedule::new(true);
    assert!(!s.due(1_000_000, None, Some(999_000)));
}

// ---------- argv constants ----------

#[test]
fn argv_exact_literals() {
    assert_eq!(CHECKUPDATES_ARGV, ["checkupdates"]);
    assert_eq!(PARU_QUA_ARGV, ["paru", "-Qua"]);
}

#[test]
fn argv_never_sudo_or_upgrade() {
    for argv in [CHECKUPDATES_ARGV.as_slice(), PARU_QUA_ARGV.as_slice()] {
        for a in argv {
            assert!(!a.contains("-S"), "{a} contains -S");
            assert!(!a.contains("-U"), "{a} contains -U");
            assert!(!a.contains("sudo"), "{a} contains sudo");
            assert!(!a.contains("--noconfirm"), "{a} contains --noconfirm");
        }
    }
}
