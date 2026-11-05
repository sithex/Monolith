use hermes_island_lib::integrations::power::model::{
    AlertConfig, AlertEngine, AlertKind, ChargeState, PowerModel,
};
use serde_json::{Map, Value};

/// Hand-rolled LCG (MMIX constants), fixed seed, no external deps.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }

    fn f(&mut self) -> f64 {
        // roughly [-1024, 1024)
        (self.below(1 << 21) as f64) / 1024.0 - 1024.0
    }

    fn value(&mut self) -> Value {
        match self.below(9) {
            0 => Value::Number(serde_json::Number::from(self.below(1_000) as i64 - 200)),
            1 => match serde_json::Number::from_f64(self.f()) {
                Some(n) => Value::Number(n),
                None => Value::Null,
            },
            2 => Value::Bool(self.coin()),
            3 => Value::String(format!("s{}", self.below(10))),
            4 => Value::Null,
            5 => Value::Number(serde_json::Number::from(self.below(2) * 100)),
            6 => Value::Number(serde_json::Number::from(self.below(2000) as i64)),
            7 => Value::Array(vec![
                Value::Bool(self.coin()),
                Value::Number(serde_json::Number::from(self.below(6))),
            ]),
            _ => Value::Object(Map::new()),
        }
    }

    fn props(&mut self) -> Map<String, Value> {
        const KEYS: [&str; 6] = [
            "Percentage",
            "State",
            "TimeToEmpty",
            "TimeToFull",
            "IsPresent",
            "junk",
        ];
        let mut m = Map::new();
        let n = self.below(8);
        for _ in 0..n {
            let k = KEYS[self.below(KEYS.len() as u64) as usize];
            m.insert(k.to_string(), self.value());
        }
        m
    }
}

#[test]
fn never_panics_and_alerts_require_intervening_rearm() {
    let mut rng = Lcg(0x243F_6A88_85A3_08D3);
    let cfg = AlertConfig::default();
    let low = cfg.low();
    let critical = cfg.critical();
    let full = cfg.full();
    let hyst = cfg.hysteresis();

    for _ in 0..500 {
        let mut model = PowerModel::default();
        let mut engine = AlertEngine::new(cfg);
        let mut last_fire = [None; 3]; // per kind: last step index
        let mut rearm_seen = [false; 3]; // re-arm observed since the last fire

        let steps = rng.below(300);
        for step in 0..steps {
            model.apply_device_props(&rng.props());
            model.apply_on_battery(&rng.value());
            let snap = model.snapshot();

            let pct = snap.percentage();
            let present = snap.present();
            let state = snap.state();
            let discharging = matches!(
                state,
                ChargeState::Discharging | ChargeState::PendingDischarge
            ) || (state == ChargeState::Unknown
                && snap.on_battery() == Some(true));
            let charging = matches!(state, ChargeState::Charging | ChargeState::PendingCharge);
            let full_cond = state == ChargeState::FullyCharged
                || ((state == ChargeState::Charging || state == ChargeState::PendingCharge)
                    && pct.is_some_and(|p| p >= full));

            // gated re-arm preconditions (same rules as the engine)
            let rearm = [
                pct.is_some() && {
                    let p = pct.unwrap_or(0.0);
                    let cond = discharging && p <= low;
                    !cond && (p >= low + hyst || charging)
                },
                pct.is_some() && {
                    let p = pct.unwrap_or(0.0);
                    let cond = discharging && p <= critical;
                    !cond && (p >= critical + hyst || charging)
                },
                pct.is_some() && {
                    let p = pct.unwrap_or(0.0);
                    !full_cond && (p <= full - hyst || discharging)
                },
            ];
            for (i, r) in rearm.iter().enumerate() {
                if *r {
                    rearm_seen[i] = true;
                }
            }

            let ev = engine.update(&snap);
            if let Some(kind) = ev {
                let idx = match kind {
                    AlertKind::Low => 0usize,
                    AlertKind::Critical => 1,
                    AlertKind::Full => 2,
                };
                // fire preconditions must hold on this snapshot
                match kind {
                    AlertKind::Low => assert!(discharging && pct.is_some_and(|p| p <= low)),
                    AlertKind::Critical => {
                        assert!(discharging && pct.is_some_and(|p| p <= critical))
                    }
                    AlertKind::Full => assert!(full_cond),
                }
                assert!(present && pct.is_some());
                if last_fire[idx].is_some() {
                    // must have re-armed since the previous fire of this kind
                    assert!(
                        rearm_seen[idx],
                        "kind {idx:?} fired twice at step {step} without an intervening re-arm"
                    );
                }
                last_fire[idx] = Some(step);
                rearm_seen[idx] = false;
            }
        }
    }
}
