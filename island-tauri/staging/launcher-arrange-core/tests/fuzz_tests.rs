use launcher_arrange_core::{ArrangeError, Arrangement, GridItem};
use std::collections::HashSet;

/// Hand-rolled LCG (Numerical Recipes constants) so the fuzz test has no
/// external dependencies.
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

fn rng(state: &mut u64, bound: u64) -> u64 {
    if bound == 0 {
        return 0;
    }
    lcg(state) % bound
}

#[derive(Clone, PartialEq, Debug)]
struct Observed {
    visible: Vec<(String, bool, bool)>,
    hidden: Vec<(String, bool, bool)>,
}

fn observe(a: &Arrangement) -> Observed {
    Observed {
        visible: a
            .visible()
            .into_iter()
            .map(|g| (g.id.clone(), g.enabled, g.locked))
            .collect(),
        hidden: a
            .hidden()
            .into_iter()
            .map(|g| (g.id.clone(), g.enabled, g.locked))
            .collect(),
    }
}

fn ids(a: &Arrangement) -> Vec<String> {
    let mut v: Vec<String> = a.visible().into_iter().map(|g| g.id.clone()).collect();
    v.extend(a.hidden().into_iter().map(|g| g.id.clone()));
    v
}

fn assert_invariants(a: &Arrangement) {
    let all = ids(a);
    let uniq: HashSet<&str> = all.iter().map(|s| s.as_str()).collect();
    assert_eq!(uniq.len(), all.len(), "ids must be unique");
    for g in a.visible().into_iter().chain(a.hidden()) {
        assert!(!g.locked || g.enabled, "locked item must be enabled");
    }
    let v: HashSet<&str> = a.visible().into_iter().map(|g| g.id.as_str()).collect();
    let h: HashSet<&str> = a.hidden().into_iter().map(|g| g.id.as_str()).collect();
    assert!(v.is_disjoint(&h), "visible and hidden must be disjoint");
    assert_eq!(
        v.len() + h.len(),
        all.len(),
        "visible+hidden must cover all ids"
    );
}

fn assert_permutation(order: &[String], a: &Arrangement) {
    let mut o = order.to_vec();
    o.sort_unstable();
    let mut e = ids(a);
    e.sort_unstable();
    assert_eq!(
        o, e,
        "commit order must be a permutation of the current id set"
    );
    assert_eq!(order.len(), e.len());
}

#[test]
fn fuzz_random_op_sequences_never_panic_and_keep_invariants() {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for round in 0..400 {
        let n = 1 + rng(&mut state, 10) as usize;
        let mut items = Vec::new();
        let mut used = HashSet::new();
        for i in 0..n {
            let id = format!("item{i}");
            if !used.insert(id.clone()) {
                continue;
            }
            let locked = rng(&mut state, 4) == 0;
            let enabled = rng(&mut state, 2) == 0 || locked;
            items.push(GridItem {
                id,
                enabled,
                locked,
            });
        }
        let mut arr = match Arrangement::new(items) {
            Ok(a) => a,
            Err(e) => panic!("new failed: {e}"),
        };
        assert_invariants(&arr);
        let mut editing = false;
        let mut snapshot_matches_work = true;
        let mut expected_snapshot = observe(&arr);
        for step in 0..80 {
            let op = rng(&mut state, 9);
            match op {
                0 => {
                    // begin_edit
                    match arr.begin_edit() {
                        Ok(()) => {
                            editing = true;
                            snapshot_matches_work = true;
                            expected_snapshot = observe(&arr);
                        }
                        Err(ArrangeError::AlreadyEditing) => assert!(editing),
                        Err(_) => panic!("begin_edit unexpected error"),
                    }
                }
                1 => {
                    // move_item
                    let all = ids(&arr);
                    if !all.is_empty() {
                        let id = all[rng(&mut state, all.len() as u64) as usize].clone();
                        let idx = rng(&mut state, 40) as usize;
                        match arr.move_item(&id, idx) {
                            Ok(()) => snapshot_matches_work = false,
                            Err(ArrangeError::NotEditing) => assert!(!editing),
                            Err(_) => {}
                        }
                    }
                }
                2 => {
                    // disable
                    let all = ids(&arr);
                    if !all.is_empty() {
                        let id = all[rng(&mut state, all.len() as u64) as usize].clone();
                        match arr.disable(&id) {
                            Ok(()) => snapshot_matches_work = false,
                            Err(ArrangeError::NotEditing) => assert!(!editing),
                            Err(_) => {}
                        }
                    }
                }
                3 => {
                    // enable
                    let all = ids(&arr);
                    if !all.is_empty() {
                        let id = all[rng(&mut state, all.len() as u64) as usize].clone();
                        match arr.enable(&id) {
                            Ok(()) => snapshot_matches_work = false,
                            Err(ArrangeError::NotEditing) => assert!(!editing),
                            Err(_) => {}
                        }
                    }
                }
                4 => {
                    // cancel
                    match arr.cancel() {
                        Ok(()) => {
                            assert!(editing);
                            if snapshot_matches_work {
                                assert_eq!(
                                    observe(&arr),
                                    expected_snapshot,
                                    "cancel must restore the snapshot"
                                );
                            }
                            editing = false;
                            snapshot_matches_work = true;
                            expected_snapshot = observe(&arr);
                        }
                        Err(ArrangeError::NotEditing) => assert!(!editing),
                        Err(_) => panic!("cancel unexpected error"),
                    }
                }
                5 => {
                    // commit
                    match arr.commit() {
                        Ok(Some(c)) => {
                            assert!(editing);
                            assert_permutation(&c.order, &arr);
                            editing = false;
                            snapshot_matches_work = true;
                            expected_snapshot = observe(&arr);
                        }
                        Ok(None) => {
                            assert!(editing);
                            editing = false;
                            snapshot_matches_work = true;
                            expected_snapshot = observe(&arr);
                        }
                        Err(ArrangeError::NotEditing) => assert!(!editing),
                        Err(_) => panic!("commit unexpected error"),
                    }
                }
                6 => {
                    // reconcile with a random subset, occasionally a new id
                    let all = ids(&arr);
                    let mut current: Vec<GridItem> = Vec::new();
                    let mut chosen: HashSet<usize> = HashSet::new();
                    if !all.is_empty() {
                        let keep = rng(&mut state, all.len() as u64 + 1) as usize;
                        for _ in 0..keep {
                            let i = rng(&mut state, all.len() as u64) as usize;
                            if chosen.insert(i) {
                                let locked = rng(&mut state, 3) == 0;
                                let enabled = rng(&mut state, 2) == 0 || locked;
                                current.push(GridItem {
                                    id: all[i].clone(),
                                    enabled,
                                    locked,
                                });
                            }
                        }
                    }
                    if rng(&mut state, 4) == 0 {
                        current.push(GridItem {
                            id: format!("new{round}_{step}"),
                            enabled: rng(&mut state, 2) == 0,
                            locked: false,
                        });
                    }
                    arr.reconcile(&current);
                    // reconcile keeps the snapshot parallel: if work matched the
                    // snapshot before, it still does afterwards.
                    if snapshot_matches_work {
                        expected_snapshot = observe(&arr);
                    }
                }
                _ => {
                    // visible/hidden sanity (also forces borrow checks)
                    let _ = observe(&arr);
                }
            }
            assert_invariants(&arr);
        }
    }
}
