use super::scan::{json_len, safe_tool_name, to_ms, utf16_len};
use super::*;
use crate::{
    contract::claude_code::{ClaudeCodeCursor, ClaudeCodeFact, ClaudeFile},
    services::scratch,
};
use serde_json::{json, Value};
use std::io::Write;
fn req(cursors: Vec<ClaudeCodeCursor>, budget_bytes: Option<f64>) -> ClaudeCodeRequest {
    ClaudeCodeRequest {
        cursors,
        budget_bytes,
    }
}
fn cursors(raw: &ClaudeCodeRaw) -> Vec<ClaudeCodeCursor> {
    raw.files
        .iter()
        .map(|f| ClaudeCodeCursor {
            key: f.key.clone(),
            ino: f.ino.clone(),
            off: f.next,
        })
        .collect()
}
fn file<'a>(r: &'a ClaudeCodeRaw, key: &str) -> &'a ClaudeFile {
    r.files.iter().find(|f| f.key == key).unwrap()
}
fn append(path: &Path, text: &str) {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}
fn asst(id: &str) -> String {
    format!(
        "{}\n",
        json!({"type":"assistant","timestamp":"2026-11-04T12:00:00Z","requestId":"r",
        "cwd":"/w/Proj","entrypoint":"cli","message":{"id":id,"model":"claude-opus-5-5","stop_reason":"end_turn",
        "content":[{"type":"text","text":"hello"}],"usage":{"input_tokens":3,"output_tokens":7}}})
    )
}
fn fixture(tag: &str) -> (PathBuf, PathBuf, ClaudeCodeService) {
    let root = scratch(tag).join("claude");
    fs::create_dir_all(root.join("projects/p/s1/subagents")).unwrap();
    let main = root.join("projects/p/s1.jsonl");
    let svc = ClaudeCodeService::new(root.clone());
    (root, main, svc)
}
#[test]
fn missing_folder_and_directory_resolution() {
    let svc = ClaudeCodeService::new(scratch("cc-missing").join("absent"));
    let raw = svc.raw(&req(vec![], None));
    assert!(raw.folder_missing && raw.claude_dir.is_none() && raw.files.is_empty() && !raw.more);
    assert_eq!(
        claude_dir(Some("".into()), Path::new("/h")),
        PathBuf::from("/h/.claude")
    );
    assert_eq!(
        claude_dir(Some("/c".into()), Path::new("/h")),
        PathBuf::from("/c")
    );
}
#[test]
fn listing_resume_append_and_partial_tail() {
    let (root, main, svc) = fixture("cc-resume");
    append(&main, &asst("m0"));
    let line = asst("m1");
    append(&main, &line[..40]);
    append(
        &root.join("projects/p/s1/subagents/agent-a.jsonl"),
        &asst("sub"),
    );
    fs::write(root.join("projects/top.jsonl"), asst("wrong-depth")).unwrap();
    std::os::unix::fs::symlink(&main, root.join("projects/p/link.jsonl")).unwrap();
    let first = svc.raw(&req(vec![], None));
    assert_eq!(first.files.len(), 2);
    let f = file(&first, "p/s1.jsonl");
    assert_eq!(f.facts.len(), 1);
    assert_eq!(f.next, f.size - 40.0);
    assert!(!first.more);
    assert!(file(&first, "p/s1/subagents/agent-a.jsonl").sub);
    append(&main, &line[40..]);
    let second = svc.raw(&req(cursors(&first), None));
    let f2 = file(&second, "p/s1.jsonl");
    assert!(!f2.reset && f2.from == f.next && f2.next == f2.size);
    assert!(matches!(&f2.facts[..], [ClaudeCodeFact::A { key, .. }] if key == "m1|r"));
    let warm = svc.raw(&req(cursors(&second), None));
    assert!(warm
        .files
        .iter()
        .all(|f| !f.reset && f.facts.is_empty() && f.from == f.next));
}
#[test]
fn shrink_inode_replacement_and_invalid_cursors_reset() {
    let (root, main, svc) = fixture("cc-reset");
    append(&main, &asst("m0"));
    let first = svc.raw(&req(vec![], None));
    fs::OpenOptions::new()
        .write(true)
        .open(&main)
        .unwrap()
        .set_len(10)
        .unwrap();
    let small = svc.raw(&req(cursors(&first), None));
    assert!(small.files[0].reset && small.files[0].next == 0.0);
    let tmp = root.join("replacement");
    fs::write(&tmp, asst("new")).unwrap();
    fs::rename(tmp, &main).unwrap();
    let replaced = svc.raw(&req(cursors(&small), None));
    assert!(replaced.files[0].reset);
    assert_ne!(replaced.files[0].ino, small.files[0].ino);
    let mut bad = cursors(&replaced);
    bad[0].off = 1.5;
    assert!(svc.raw(&req(bad, None)).files[0].reset);
}
#[test]
fn budgets_list_untouched_files_and_eventually_drain() {
    let (root, _, svc) = fixture("cc-budget");
    for i in 0..4 {
        for j in 0..10 {
            append(
                &root.join(format!("projects/p/s{i}.jsonl")),
                &asst(&format!("{i}-{j}")),
            );
        }
    }
    let mut cur = vec![];
    let mut facts = 0;
    let mut calls = 0;
    loop {
        let raw = svc.raw(&req(cur, Some(1.0)));
        calls += 1;
        assert_eq!(raw.files.len(), 4);
        facts += raw.files.iter().map(|f| f.facts.len()).sum::<usize>();
        cur = cursors(&raw);
        if !raw.more {
            break;
        }
        assert!(calls <= 40);
    }
    assert_eq!(facts, 40);
    let zero = svc.raw(&req(vec![], Some(0.0)));
    assert!(
        zero.more
            && zero
                .files
                .iter()
                .all(|f| f.next == 0.0 && f.facts.is_empty())
    );
}
#[test]
fn huge_line_bounded_and_partial_not_consumed() {
    let (_, main, svc) = fixture("cc-huge");
    append(&main, &"x".repeat(MAX_LINE + 10));
    let partial = svc.raw(&req(vec![], None));
    assert_eq!(partial.files[0].next, 0.0);
    assert_eq!(partial.files[0].skipped_huge, 0.0);
    append(&main, "\n");
    append(&main, &asst("after"));
    let raw = svc.raw(&req(cursors(&partial), None));
    assert_eq!(raw.files[0].skipped_huge, 1.0);
    assert_eq!(raw.files[0].facts.len(), 1);
    assert_eq!(raw.files[0].next, raw.files[0].size);
}
#[test]
fn serialized_result_never_contains_secret_fixture_content() {
    let (root, main, svc) = fixture("cc-secret");
    const SECRET: &str = "SECRET_FIXTURE_DO_NOT_SEND_7f8dd6";
    let mut v: Value = serde_json::from_str(&asst("m1")).unwrap();
    v["message"]["content"] = json!([
        {"type":"text","text":SECRET}, {"type":"thinking","thinking":SECRET,"signature":SECRET},
        {"type":"tool_use","name":"Bash","input":{"command":SECRET}},
        {"type":"tool_result","content":SECRET}
    ]);
    append(&main, &format!("{v}\n"));
    append(
        &main,
        &format!("{}\n", json!({"type":"user","message":{"content":SECRET}})),
    );
    append(
        &main,
        &format!(
            "{}\n",
            json!({"type":"system","subtype":"turn_duration","content":SECRET})
        ),
    );
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::write(
        root.join("sessions/1.json"),
        json!({"sessionId":"s1","status":"busy","name":SECRET,
        "message":SECRET,"content":SECRET,"extra":{"secret":SECRET},"startedAt":SECRET})
        .to_string(),
    )
    .unwrap();
    fs::write(root.join(".credentials.json"), SECRET).unwrap();
    fs::write(root.join("sessions/1.key"), SECRET).unwrap();
    let raw = svc.raw(&req(vec![], None));
    assert_eq!(raw.files[0].facts.len(), 3);
    assert_eq!(raw.registry[0].data["sessionId"], "s1");
    assert!(raw.registry[0].data.get("name").is_none());
    let serialized = serde_json::to_string(&raw).unwrap();
    assert!(
        !serialized.contains(SECRET),
        "secret content crossed the IPC boundary"
    );
    assert!(
        !serialized.contains("command")
            && !serialized.contains("thinking")
            && !serialized.contains("tool_result")
    );
    assert_eq!(
        fs::read_to_string(root.join(".credentials.json")).unwrap(),
        SECRET
    );
}
#[test]
fn registry_pid_identity_and_symlink_roots_are_rejected() {
    let (root, _, svc) = fixture("cc-reg");
    fs::create_dir_all(root.join("sessions")).unwrap();
    let me = std::process::id();
    fs::write(
        root.join(format!("sessions/{me}.json")),
        "{\"sessionId\":\"s1\",\"status\":\"busy\"}",
    )
    .unwrap();
    fs::write(root.join("sessions/4000000000.json"), "bad json").unwrap();
    std::os::unix::fs::symlink(
        root.join(format!("sessions/{me}.json")),
        root.join("sessions/2.json"),
    )
    .unwrap();
    let raw = svc.raw(&req(vec![], None));
    assert_eq!(raw.registry.len(), 2);
    let mine = raw
        .registry
        .iter()
        .find(|e| e.file == format!("{me}.json"))
        .unwrap();
    assert!(mine.alive && mine.proc_start.is_some());
    let alias = root.with_extension("link");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    assert!(ClaudeCodeService::new(alias)
        .raw(&req(vec![], None))
        .error
        .is_some());
    fs::rename(root.join("projects"), root.join("other")).unwrap();
    std::os::unix::fs::symlink(root.join("other"), root.join("projects")).unwrap();
    let raw = svc.raw(&req(vec![], None));
    assert!(raw.files.is_empty() && raw.error.is_some());
}
#[test]
fn whitespace_prefilters_facts_and_bad_lines() {
    let (_, main, svc) = fixture("cc-facts");
    append(&main, &asst("a"));
    append(
        &main,
        "{ \"type\" : \"user\", \"isMeta\":true, \"message\":{\"content\":\"x\"}}\n",
    );
    append(
        &main,
        "{\"type\":\"user\",\"message\":{\"content\":\"[Request interrupted by user]\"}}\n",
    );
    append(&main, "{\"type\":\"system\",\"subtype\":\"compact_boundary\"}\n{\"type\":\"system\",\"subtype\":\"turn_duration\"}\n{\"type\":\"system\",\"subtype\":\"info\"}\n");
    append(&main, "{\"type\":\"assistant\", broken\n");
    let raw = svc.raw(&req(vec![], None));
    let f = &raw.files[0];
    assert_eq!(f.bad_lines, 1.0);
    assert_eq!(f.facts.len(), 6);
    let facts = serde_json::to_value(&f.facts).unwrap();
    assert_eq!(facts[1]["meta"], true);
    assert_eq!(facts[2]["interrupt"], true);
    assert_eq!(facts[3]["t"], "compact");
    assert_eq!(facts[4]["t"], "turnEnd");
    assert_eq!(facts[5]["t"], "sys");
}
#[test]
fn huge_line_checkpoints_bound_each_poll_and_resume_without_parsing_a_tail() {
    let root = scratch("claude-huge-budget");
    let project = root.join("projects/p");
    fs::create_dir_all(&project).unwrap();
    let path = project.join("s.jsonl");
    fs::File::create(&path)
        .unwrap()
        .set_len((MAX_LINE * 3) as u64)
        .unwrap();
    append(&path, &format!("\n{}", asst("after-huge")));
    let service = ClaudeCodeService::new(root.clone());
    let mut positions = vec![];
    let (mut facts, mut skipped, mut done) = (0, 0., false);
    for _ in 0..8 {
        let raw = service.raw(&req(positions, Some(1.)));
        let f = &raw.files[0];
        assert!(f.next > f.from, "each bounded poll makes progress");
        assert!(f.next - f.from <= (MAX_LINE + 2) as f64);
        facts += f.facts.len();
        skipped += f.skipped_huge;
        positions = cursors(&raw);
        if !raw.more {
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(facts, 1);
    assert_eq!(skipped, 1.);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn partial_tails_consume_the_shared_io_budget_instead_of_reading_every_file() {
    let root = scratch("claude-tail-budget");
    let project = root.join("projects/p");
    fs::create_dir_all(&project).unwrap();
    for name in ["a", "b", "c"] {
        fs::write(project.join(format!("{name}.jsonl")), "{partial").unwrap();
    }
    let service = ClaudeCodeService::new(root.clone());
    let raw = service.raw(&req(vec![], Some(1.)));
    assert!(raw.more, "unread files yield to another visible poll");
    assert!(raw.files.iter().all(|f| f.facts.is_empty() && f.next == 0.));
    let second = service.raw(&req(cursors(&raw), Some(1.)));
    let third = service.raw(&req(cursors(&second), Some(1.)));
    assert!(
        !third.more,
        "unchanged partial tails cannot starve later files"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lengths_tools_and_timestamps_match_js() {
    assert_eq!(utf16_len("a😀é"), 4);
    assert_eq!(
        safe_tool_name(Some(&json!("mcp__a:b.c-d"))).as_deref(),
        Some("mcp__a:b.c-d")
    );
    assert_eq!(
        safe_tool_name(Some(&json!("bad name!"))).as_deref(),
        Some("tool")
    );
    for (v, s) in [
        (
            json!([1.5e-6, 1e-7, 1e21, -0.0]),
            "[0.0000015,1e-7,1e+21,0]",
        ),
        (json!({"x":"😀\n"}), "{\"x\":\"😀\\n\"}"),
    ] {
        assert_eq!(json_len(&v), utf16_len(s));
    }
    assert_eq!(
        to_ms(&Some(json!("2026-11-04T12:00:00Z"))),
        Some(1_793_793_600_000.0)
    );
    assert_eq!(
        to_ms(&Some(json!("1793793600123"))),
        Some(1_793_793_600_123.0)
    );
    assert_eq!(to_ms(&Some(json!("not a timestamp"))), None);
}
