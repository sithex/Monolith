//! Tile assembly: combine parsed inputs into the tile models.

use crate::integrations::tiles::core::model::{
    NetKind, NetworkTile, NmConnectivity, NmSnapshot, NmState, OutputTile, Sink,
};

/// Assemble the audio output tile. `choices` keep pactl order; `current` is
/// the sink whose name equals `default_name`, `None` when absent (or when no
/// default name was read).
pub fn output_tile(sinks: Vec<Sink>, default_name: Option<String>) -> OutputTile {
    let current = match default_name {
        Some(name) => sinks.iter().find(|s| s.name == name).cloned(),
        None => None,
    };
    OutputTile {
        current,
        choices: sinks,
    }
}

/// Assemble the Network tile from a NetworkManager snapshot (or `None` when
/// the D-Bus service is unavailable). Returns the tile and the number of
/// warnings (unknown numeric `State`/`Connectivity` values are `None` and
/// counted).
pub fn network_tile_report(nm: Option<NmSnapshot>) -> (NetworkTile, u32) {
    let snapshot = match nm {
        Some(snapshot) => snapshot,
        None => {
            return (
                NetworkTile {
                    available: false,
                    reason: Some("NetworkManager D-Bus service unavailable".to_string()),
                    state: None,
                    connectivity: None,
                    kind: NetKind::None,
                    show_wifi_toggle: false,
                    wifi_enabled: None,
                },
                0,
            );
        }
    };

    let mut warnings = 0u32;
    let state = snapshot.state.and_then(|v| match NmState::from_u32(v) {
        Some(s) => Some(s),
        None => {
            warnings += 1;
            None
        }
    });
    let connectivity = snapshot
        .connectivity
        .and_then(|v| match NmConnectivity::from_u32(v) {
            Some(c) => Some(c),
            None => {
                warnings += 1;
                None
            }
        });
    let kind = match snapshot.primary_connection_type.as_deref() {
        Some("802-3-ethernet") => NetKind::Wired,
        Some("802-11-wireless") => NetKind::Wifi,
        Some("") | None => NetKind::None,
        Some(_) => NetKind::Other,
    };
    // Wi-Fi is effectively enabled only when the software toggle is on and
    // the hardware switch is not off.
    let wifi_enabled = if snapshot.wireless_hardware_enabled == Some(false) {
        Some(false)
    } else {
        snapshot.wireless_enabled
    };

    let tile = NetworkTile {
        available: true,
        reason: None,
        state,
        connectivity,
        kind,
        show_wifi_toggle: snapshot.has_wireless_device,
        wifi_enabled,
    };
    (tile, warnings)
}

/// Assemble the Network tile; unknown numeric values are `None` (warnings
/// are dropped here — see [`network_tile_report`] for the count).
pub fn network_tile(nm: Option<NmSnapshot>) -> NetworkTile {
    network_tile_report(nm).0
}
