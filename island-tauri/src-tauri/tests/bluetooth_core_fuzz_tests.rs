use hermes_island_lib::integrations::bluetooth::model::{BtDevice, BtModel};
use serde_json::{Map, Value};
use std::collections::HashSet;

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

    fn pick<'a>(&mut self, xs: &'a [&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }

    /// A random JSON value, mostly malformed for our property keys.
    fn value(&mut self) -> Value {
        match self.below(11) {
            0 => Value::String(format!("p{}", self.below(8))),
            1 => Value::String(format!("AA:BB:CC:DD:EE:{:02X}", self.below(256))),
            2 => Value::Number(serde_json::Number::from(self.below(300) as i64 - 150)),
            3 => Value::Number(serde_json::Number::from(self.below(70_000) as i64 - 35_000)),
            4 => Value::Bool(self.coin()),
            5 => Value::Null,
            6 => {
                let f = (self.below(2000) as f64) / 8.0 - 100.0;
                Value::Number(
                    serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
                )
            }
            7 => Value::Array(
                (0..self.below(4))
                    .map(|_| Value::String(format!("u{}", self.below(5))))
                    .collect(),
            ),
            8 => Value::Array(vec![
                Value::Bool(self.coin()),
                Value::Number(serde_json::Number::from(self.below(300))),
            ]),
            9 => Value::Object(Map::new()),
            _ => Value::String(format!("s{}", self.below(10))),
        }
    }

    /// A random object path: a mix of D-Bus-valid and invalid strings.
    fn path(&mut self) -> String {
        let mut s = String::new();
        let n = self.below(5);
        for _ in 0..n {
            s.push('/');
            let seg = self.below(4);
            for _ in 0..seg {
                s.push_str(self.pick(&["a", "B", "1", "_", "-", ".", " "]));
            }
        }
        if self.coin() {
            s.push('/');
        }
        if s.is_empty() {
            s.push('/');
        }
        s
    }

    fn props(&mut self) -> Value {
        let mut obj = Map::new();
        let n = self.below(8);
        for _ in 0..n {
            let key = self.pick(&[
                "Address",
                "Name",
                "Alias",
                "Icon",
                "Paired",
                "Bonded",
                "Connected",
                "Trusted",
                "Blocked",
                "Adapter",
                "RSSI",
                "ServicesResolved",
                "UUIDs",
                "Percentage",
                "Powered",
                "Discovering",
                "junk",
            ]);
            obj.insert(key.to_string(), self.value());
        }
        Value::Object(obj)
    }

    fn ifaces(&mut self) -> Value {
        let mut obj = Map::new();
        let n = self.below(4);
        for _ in 0..n {
            let iface = self.pick(&[
                "org.bluez.Device1",
                "org.bluez.Battery1",
                "org.bluez.Adapter1",
                "org.bluez.Media1",
                "junk",
            ]);
            obj.insert(iface.to_string(), self.props());
        }
        Value::Object(obj)
    }

    fn blob(&mut self) -> Value {
        let mut obj = Map::new();
        let n = self.below(6);
        for _ in 0..n {
            let p = self.path();
            obj.insert(p, self.ifaces());
        }
        Value::Object(obj)
    }
}

/// Assert the model invariants that must hold after any sequence of input:
/// unique paths, the documented device ordering, adapters sorted by path,
/// and summary consistency with `devices()` / `adapters()`.
fn check_invariants(m: &BtModel) {
    let devices = m.devices();
    let mut seen: HashSet<String> = HashSet::new();
    for w in devices.windows(2) {
        let a = &w[0];
        let b = &w[1];
        assert!(seen.insert(a.path.clone()), "duplicate path {}", a.path);
        if a.connected != b.connected {
            assert!(
                a.connected,
                "connected must sort first: {} before {}",
                a.path, b.path
            );
        } else if a.paired != b.paired {
            assert!(
                a.paired,
                "paired must sort before unpaired: {} before {}",
                a.path, b.path
            );
        } else {
            let la = a.display_name.to_lowercase();
            let lb = b.display_name.to_lowercase();
            if la != lb {
                assert!(
                    la < lb,
                    "displayName order violated: {} vs {}",
                    a.display_name,
                    b.display_name
                );
            } else {
                assert!(
                    a.path < b.path,
                    "path tiebreak violated: {} vs {}",
                    a.path,
                    b.path
                );
            }
        }
    }
    if let Some(last) = devices.last() {
        seen.insert(last.path.clone());
    }
    assert_eq!(seen.len(), devices.len(), "non-unique device paths");

    let adapters = m.adapters();
    for w in adapters.windows(2) {
        assert!(w[0].path < w[1].path, "adapters not sorted by path");
    }

    let s = m.summary();
    let connected: Vec<&BtDevice> = devices.iter().filter(|d| d.connected).collect();
    assert_eq!(s.connected_count, connected.len());
    let names: Vec<String> = connected
        .iter()
        .take(3)
        .map(|d| d.display_name.clone())
        .collect();
    assert_eq!(s.connected_names, names);
    let lowest = connected.iter().filter_map(|d| d.battery_percent).min();
    assert_eq!(s.lowest_battery_percent, lowest);
    assert_eq!(s.powered_any, adapters.iter().any(|a| a.powered));
}

#[test]
fn never_panics_and_invariants_hold() {
    let mut rng = Lcg(0xF00D_F00D_CAFE_1234);
    for _ in 0..300 {
        let mut model = BtModel::default();
        let steps = rng.below(120);
        for _ in 0..steps {
            match rng.below(5) {
                0 => {
                    let blob = rng.blob();
                    let _ = model.load_managed_objects(&blob);
                }
                1 => {
                    let p = rng.path();
                    let ifaces = rng.ifaces();
                    model.interfaces_added(&p, &ifaces);
                }
                2 => {
                    let p = rng.path();
                    let mut names = Vec::new();
                    let n = rng.below(4);
                    for _ in 0..n {
                        names.push(
                            rng.pick(&[
                                "org.bluez.Device1",
                                "org.bluez.Battery1",
                                "org.bluez.Adapter1",
                                "x",
                            ])
                            .to_string(),
                        );
                    }
                    model.interfaces_removed(&p, &names);
                }
                3 => {
                    let p = rng.path();
                    let iface = rng.pick(&[
                        "org.bluez.Device1",
                        "org.bluez.Battery1",
                        "org.bluez.Adapter1",
                        "junk",
                    ]);
                    let mut changed = Map::new();
                    let n = rng.below(6);
                    for _ in 0..n {
                        let k = rng.pick(&[
                            "Address",
                            "Name",
                            "Alias",
                            "Icon",
                            "Paired",
                            "Bonded",
                            "Connected",
                            "Trusted",
                            "Blocked",
                            "Adapter",
                            "RSSI",
                            "ServicesResolved",
                            "UUIDs",
                            "Percentage",
                            "Powered",
                            "Discovering",
                            "junk",
                        ]);
                        changed.insert(k.to_string(), rng.value());
                    }
                    let mut invalidated = Vec::new();
                    let n = rng.below(3);
                    for _ in 0..n {
                        invalidated.push(
                            rng.pick(&[
                                "Connected",
                                "Percentage",
                                "Powered",
                                "Alias",
                                "RSSI",
                                "Address",
                            ])
                            .to_string(),
                        );
                    }
                    model.properties_changed(&p, iface, &changed, &invalidated);
                }
                _ => check_invariants(&model),
            }
        }
        check_invariants(&model);
    }
}
