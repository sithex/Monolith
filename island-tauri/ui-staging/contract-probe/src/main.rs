use launcher_arrange_core::{Arrangement, GridItem};
use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut traces = Vec::new();
    for seed in 0..16_u64 {
        let initial = vec![
            GridItem {
                id: "terminal".into(),
                enabled: false,
                locked: true,
            },
            GridItem {
                id: "hidden".into(),
                enabled: false,
                locked: false,
            },
            GridItem {
                id: "timer".into(),
                enabled: true,
                locked: false,
            },
            GridItem {
                id: "ai".into(),
                enabled: true,
                locked: false,
            },
            GridItem {
                id: "settings".into(),
                enabled: true,
                locked: true,
            },
            GridItem {
                id: "future".into(),
                enabled: false,
                locked: false,
            },
        ];
        let mut a = Arrangement::new(initial.clone())?;
        let mut steps = Vec::new();
        let mut rng = seed + 1;
        let mut actions = vec![
            json!({"type":"begin"}),
            json!({"type":"disable","id":"terminal"}),
            json!({"type":"enable","id":"hidden"}),
            json!({"type":"move","id":"settings","to":0}),
            json!({"type":"commit"}),
            json!({"type":"begin"}),
            json!({"type":"disable","id":"ai"}),
            json!({"type":"cancel"}),
        ];
        let ids = [
            "terminal", "hidden", "timer", "ai", "settings", "future", "missing",
        ];
        for _ in 0..64 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let id = ids[(rng >> 32) as usize % ids.len()];
            let action = match rng % 9 {
                0 => json!({"type":"begin"}),
                1 => json!({"type":"commit"}),
                2 => json!({"type":"cancel"}),
                3 => json!({"type":"disable","id":id}),
                4 => json!({"type":"enable","id":id}),
                5 => {
                    json!({"type":"reconcile","items":[{"id":"terminal","locked":true,"enabled":true},{"id":"timer","locked":rng % 2 == 0,"enabled":true},{"id":"settings","locked":true,"enabled":false},{"id":"new","locked":false,"enabled":true}]})
                }
                _ => json!({"type":"move","id":id,"to":(rng >> 40) % 12}),
            };
            actions.push(action);
        }
        // End with a changed commit so every trace exercises a full permutation.
        actions.extend([
            json!({"type":"cancel"}),
            json!({"type":"begin"}),
            json!({"type":"move","id":"settings","to":0}),
            json!({"type":"commit"}),
        ]);
        for action in actions {
            let mut commit = None;
            let id = action.get("id").and_then(Value::as_str).unwrap_or("");
            let result = match action["type"].as_str().unwrap_or("") {
                "begin" => a.begin_edit(),
                "cancel" => a.cancel(),
                "commit" => a.commit().map(|c| commit = c),
                "disable" => a.disable(id),
                "enable" => a.enable(id),
                "move" => a.move_item(id, action["to"].as_u64().unwrap_or(0) as usize),
                "reconcile" => {
                    let items: Vec<GridItem> = serde_json::from_value(action["items"].clone())?;
                    a.reconcile(&items);
                    Ok(())
                }
                _ => unreachable!(),
            };
            steps.push(json!({"action":action,"expected":{"visible":a.visible(),"hidden":a.hidden(),"editing":a.is_editing(),"commit":commit,"error":result.err().map(|e| format!("{e:?}"))}}));
        }
        traces.push(json!({"initial":initial,"steps":steps}));
    }
    println!("{}", serde_json::to_string_pretty(&traces)?);
    Ok(())
}
