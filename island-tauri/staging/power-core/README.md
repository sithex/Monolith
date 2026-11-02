# power-core

Pure Rust core behind SPEC §3.5's "Battery/power" and "Battery alerts" rows
for the Hermes Island widget. UPower D-Bus values (`org.freedesktop.UPower`
`DisplayDevice` `PropertiesChanged`, plus the root `OnBattery` property) arrive
already converted to `serde_json::Value`; nothing in this crate reads a clock.

No IO, no network, no threads, no `unsafe`, never panics on any input.

## API

- `ChargeState`, `parse_state(&Value) -> Option<ChargeState>` — UPower state
  integers 0..=6; anything else is `None`.
- `PowerModel` — `apply_device_props(&Map<String, Value>)` (PropertiesChanged
  semantics: only present keys change; an invalid value clears the field rather
  than keeping it stale), `apply_on_battery(&Value)`, `snapshot() ->
  PowerSnapshot`. Starts absent: `present=false`, everything `None`/`Unknown`.
- `PowerSnapshot` — `present`, `percentage` (`None` unless finite and inside
  0..=100), `state`, `onBattery`, `timeToEmptyS`/`timeToFullS` (`None` when 0,
  negative, non-integer or missing); never fabricates zeros. Serde wires
  camelCase; no aliases are accepted.
- `AlertConfig` — thresholds (default low 20, critical 10, full 100) and
  hysteresis (default 2); validated: `critical < low < full`, all inside
  0..=100, hysteresis inside 0..=10, else a typed `AlertError`.
- `AlertEngine::new(config)`, `update(&PowerSnapshot) -> Option<AlertKind>` —
  low / critical / full battery alerts, each fires once until re-armed
  (rules and re-arm conditions documented in `src/lib.rs` and `VERIFICATION.md`).
- `AlertKind`, `AlertError`.

Semantics are documented on the items in `src/lib.rs`; the gate output and the
mutation record live in `VERIFICATION.md`.
