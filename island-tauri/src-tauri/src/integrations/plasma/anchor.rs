//! Fixture-testable Plasma discovery and below-panel placement. No live query is
//! performed by parsing or placement; discover() explicitly runs bounded helpers
//! on a blocking worker, never on the GTK main thread.
use crate::error::IslandError;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub fn validate(self) -> Result<Self, IslandError> {
        if ![self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            || self.x.abs() > 1_000_000.
            || self.y.abs() > 1_000_000.
            || !(1. ..=32768.).contains(&self.width)
            || !(1. ..=32768.).contains(&self.height)
        {
            return Err(IslandError::invalid("Invalid screen or anchor geometry"));
        }
        Ok(self)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub output: String,
    pub panel: u32,
    pub widget: u32,
    pub native: bool,
    pub center: f64,
    pub top: f64,
    pub panel_height: f64,
    pub left: f64,
    pub screen_width: f64,
    pub screen_height: f64,
    pub screen: Rect,
    /// Absolute screen coordinates, not popup-relative coordinates.
    pub pill: Rect,
    pub pill_radius: f64,
}

/// Startup discovery snapshot used by window placement; no compositor access.
pub struct PlasmaAnchor(pub Option<Anchor>);
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
fn number(value: &Value) -> Result<f64, IslandError> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| IslandError::invalid("Missing finite anchor coordinate"))
}
fn id(value: &Value) -> Result<u32, IslandError> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| IslandError::invalid("Missing anchor identifier"))
}
const WIDGET_KINDS: &[&str] = &[
    // notch3 = adjustable width (Settings → Notch → Width); fresh id busts Plasma's QML cache.
    "local.hermes.island.notch3",
    // notch2 = same notch plus the music indicator (fresh id busts Plasma's QML cache).
    "local.hermes.island.notch2",
    "local.hermes.island.notch",
    "local.hermes.island.v3",
    "local.hermes.island",
    "local.hermes.island.panel",
    "local.hermes.island.anchor",
    "org.kde.plasma.digitalclock",
];
/// Prefer the native widget across ALL eligible panels, not merely within the
/// first panel. Invalid candidates are ignored; no valid candidate is an error.
pub fn anchor_from_data(outputs: &Value, panels: &Value) -> Result<Anchor, IslandError> {
    let output = outputs["outputs"]
        .as_array()
        .and_then(|outputs| {
            outputs
                .iter()
                .find(|o| o["connected"] == true && o["enabled"] != false && o["priority"] == 1)
        })
        .ok_or_else(|| IslandError::unavailable("No connected primary output"))?;
    let screen = Rect {
        x: number(&output["pos"]["x"])?,
        y: number(&output["pos"]["y"])?,
        width: number(&output["size"]["width"])?,
        height: number(&output["size"]["height"])?,
    }
    .validate()?;
    let name = output["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.len() <= 256)
        .ok_or_else(|| IslandError::invalid("Missing primary output name"))?;
    let panels = panels
        .as_array()
        .ok_or_else(|| IslandError::invalid("Missing Plasma panels"))?;
    for kind in WIDGET_KINDS {
        for panel in panels {
            if panel["location"] != "top" {
                continue;
            }
            let parsed = (|| -> Result<Option<Anchor>, IslandError> {
                let x = number(&panel["x"])?;
                let y = number(&panel["y"])?;
                let screen_x = panel.get("screenX").map(number).transpose()?.unwrap_or(x);
                let screen_y = panel.get("screenY").map(number).transpose()?.unwrap_or(y);
                if screen_x != screen.x || screen_y != screen.y {
                    return Ok(None);
                }
                let height = number(&panel["height"])?;
                if height < 1.
                    || height >= screen.height
                    || y < screen.y
                    || y + height >= screen.y + screen.height
                {
                    return Ok(None);
                }
                let Some(widgets) = panel["widgets"].as_array() else {
                    return Ok(None);
                };
                for widget in widgets.iter().filter(|w| w["type"] == *kind) {
                    let candidate = (|| -> Result<Anchor, IslandError> {
                        let pill = Rect {
                            x: x + number(&widget["x"])?,
                            y: y + widget.get("y").map(number).transpose()?.unwrap_or(0.),
                            width: number(&widget["width"])?,
                            height: widget
                                .get("height")
                                .map(number)
                                .transpose()?
                                .unwrap_or(height),
                        }
                        .validate()?;
                        // Panel::height() is nominal thickness, but Widget::geometry()
                        // is scene-relative and includes the floating outer inset.
                        // Keep the top within the nominal strip and the widget no
                        // thicker than it; only floating panels may extend below it.
                        let floating = panel["floating"] == true;
                        if pill.x < screen.x
                            || pill.x + pill.width > screen.x + screen.width
                            || pill.y < y
                            || pill.height > height
                            || pill.y >= y + height
                            || (!floating && pill.y + pill.height > y + height)
                            || pill.y + pill.height > screen.y + screen.height
                        {
                            return Err(IslandError::invalid(
                                "Pill is outside its primary top panel",
                            ));
                        }
                        Ok(Anchor {
                            output: name.into(),
                            panel: id(&panel["id"])?,
                            widget: id(&widget["id"])?,
                            native: *kind != "org.kde.plasma.digitalclock",
                            center: pill.x + pill.width / 2.,
                            top: y,
                            panel_height: height.max(pill.y + pill.height - y),
                            left: screen.x,
                            screen_width: screen.width,
                            screen_height: screen.height,
                            screen,
                            pill,
                            pill_radius: (pill.height / 2.).min(pill.width / 2.),
                        })
                    })();
                    if let Ok(anchor) = candidate {
                        return Ok(Some(anchor));
                    }
                }
                Ok(None)
            })();
            if let Ok(Some(anchor)) = parsed {
                return Ok(anchor);
            }
        }
    }
    Err(IslandError::unavailable(
        "No valid clock or native anchor on primary top panel",
    ))
}
/// SPEC §2.3: popup starts BELOW the panel. Small screens shrink the viewport
/// rather than moving it over the panel or letting its bounds escape the screen.
pub fn below_panel(anchor: &Anchor, width: u32, height: u32) -> Result<Placement, IslandError> {
    let screen = anchor.screen.validate()?;
    if width == 0
        || height == 0
        || !anchor.center.is_finite()
        || !anchor.top.is_finite()
        || !anchor.panel_height.is_finite()
        || anchor.panel_height < 1.
    {
        return Err(IslandError::invalid("Invalid placement input"));
    }
    let bottom = anchor.top + anchor.panel_height;
    let y = bottom.ceil();
    let available = (screen.y + screen.height).floor() - y;
    let left = screen.x.ceil();
    let right = (screen.x + screen.width).floor();
    if bottom < screen.y || available < 1. || right - left < 1. {
        return Err(IslandError::invalid("Panel leaves no popup work area"));
    }
    let width = width.min((right - left) as u32);
    let height = height.min(available as u32);
    let x = (anchor.center - f64::from(width) / 2.)
        .round()
        .clamp(left, right - f64::from(width));
    Ok(Placement {
        x: x as i32,
        y: y as i32,
        width,
        height,
    })
}

#[cfg(target_os = "linux")]
pub const DISCOVERY_SCRIPT: &str = "print(JSON.stringify(panels().map(p=>{var g=screenGeometry(p.screen);return {id:p.id,location:p.location,height:p.height,floating:p.floating,x:g.x+(p.offset||0),y:g.y,screenX:g.x,screenY:g.y,widgets:p.widgets().map(w=>{var r=w.geometry;return {id:w.id,type:w.type,x:r.x,y:r.y,width:r.width,height:r.height}})}})))";
#[cfg(target_os = "linux")]
pub fn discovery_commands() -> [std::process::Command; 2] {
    let mut outputs = std::process::Command::new("kscreen-doctor");
    outputs.arg("-j");
    let mut panels = std::process::Command::new("qdbus6");
    panels.args([
        "org.kde.plasmashell",
        "/PlasmaShell",
        "org.kde.PlasmaShell.evaluateScript",
        DISCOVERY_SCRIPT,
    ]);
    [outputs, panels]
}
#[cfg(target_os = "linux")]
pub async fn discover() -> Result<Anchor, IslandError> {
    tokio::task::spawn_blocking(|| {
        let [outputs, panels] = discovery_commands();
        let run = |command| {
            crate::services::process::run_bounded(
                command,
                std::time::Duration::from_secs(3),
                1024 * 1024,
                false,
            )
        };
        let outputs: Value = serde_json::from_slice(&run(outputs)?)
            .map_err(|_| IslandError::invalid("Invalid display helper JSON"))?;
        let panels: Value = serde_json::from_slice(&run(panels)?)
            .map_err(|_| IslandError::invalid("Invalid Plasma helper JSON"))?;
        anchor_from_data(&outputs, &panels)
    })
    .await
    .map_err(|_| IslandError::internal("Anchor discovery worker failed"))?
}
#[cfg(not(target_os = "linux"))]
pub async fn discover() -> Result<Anchor, IslandError> {
    Err(IslandError::unsupported("Plasma discovery requires Linux"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn outputs() -> Value {
        json!({"outputs":[{"connected":true,"enabled":true,"priority":1,"name":"DP-1","pos":{"x":1920,"y":0},"size":{"width":2560,"height":1440}}]})
    }
    fn panels() -> Value {
        json!([
            {"id":1,"x":0,"y":0,"height":32,"location":"top","widgets":[{"id":11,"type":"local.hermes.island.v3","x":1,"width":150}]},
            {"id":2,"x":1920,"y":0,"height":32,"location":"top","widgets":[{"id":21,"type":"org.kde.plasma.digitalclock","x":100,"width":80}]},
            {"id":3,"x":1920,"y":0,"height":32,"location":"bottom","widgets":[{"id":31,"type":"local.hermes.island.v3","x":100,"width":150}]},
            {"id":4,"x":1920,"y":0,"height":32,"location":"top","widgets":[{"id":41,"type":"local.hermes.island.v3","x":0,"width":0},{"id":42,"type":"local.hermes.island.v3","x":1200,"width":160,"height":28,"y":2}]}
        ])
    }
    #[test]
    fn notch_widget_is_a_native_anchor_not_a_digitalclock_fallback() {
        let mut data = panels();
        data[3]["widgets"][1]["type"] = json!("local.hermes.island.notch");
        let anchor = anchor_from_data(&outputs(), &data).unwrap();
        assert_eq!(anchor.widget, 42);
        assert!(anchor.native);
    }
    #[test]
    fn anchor_prefers_native_widget_on_primary_top_panel() {
        let anchor = anchor_from_data(&outputs(), &panels()).unwrap();
        assert_eq!(anchor.widget, 42);
        assert_eq!(anchor.panel, 4);
        assert_eq!(anchor.center, 3200.);
        assert!(anchor.native);
        assert_eq!(
            anchor.pill,
            Rect {
                x: 3120.,
                y: 2.,
                width: 160.,
                height: 28.
            }
        );
        assert!(anchor_from_data(&json!({"outputs":[]}), &panels()).is_err());
    }
    #[test]
    fn floating_panel_inset_does_not_hide_the_native_anchor() {
        // Plasma's height is PanelView::thickness(), while widget geometry is
        // mapToScene(): a floating panel's scene includes its outer inset.
        let mut panels = json!([{"id":145,"x":1920,"y":0,"height":32,
            "screenX":1920,"screenY":0,"location":"top","floating":true,
            "widgets":[{"id":380,"type":"local.hermes.island.v3",
                "x":1209,"y":11,"width":144,"height":26}]}]);
        let anchor = anchor_from_data(&outputs(), &panels).unwrap();
        assert_eq!(anchor.pill.y, 11.);
        assert_eq!(anchor.pill.height, 26.);
        assert_eq!(anchor.center, 3201.);
        assert!(anchor.panel_height >= anchor.pill.y + anchor.pill.height);
        panels[0]["floating"] = json!(false);
        assert!(anchor_from_data(&outputs(), &panels).is_err());
        panels[0]["floating"] = json!(true);
        panels[0]["widgets"][0]["y"] = json!(33);
        assert!(anchor_from_data(&outputs(), &panels).is_err());
        panels[0]["widgets"][0]["y"] = json!(11);
        panels[0]["widgets"][0]["height"] = json!(33);
        assert!(anchor_from_data(&outputs(), &panels).is_err());
        assert!(DISCOVERY_SCRIPT.contains("floating:p.floating"));
    }
    #[test]
    fn placement_is_below_panel_and_clamped_to_primary_screen() {
        let mut anchor = anchor_from_data(&outputs(), &panels()).unwrap();
        assert_eq!(
            below_panel(&anchor, 1180, 820).unwrap(),
            Placement {
                x: 2610,
                y: 32,
                width: 1180,
                height: 820
            }
        );
        anchor.center = 1930.;
        assert_eq!(below_panel(&anchor, 1180, 820).unwrap().x, 1920);
        anchor.center = 4479.;
        assert_eq!(below_panel(&anchor, 1180, 820).unwrap().x, 3300);
        assert_eq!(
            below_panel(&anchor, 4000, 4000).unwrap(),
            Placement {
                x: 1920,
                y: 32,
                width: 2560,
                height: 1408
            }
        );
        anchor.center = f64::NAN;
        assert!(below_panel(&anchor, 1180, 820).is_err());
    }
    #[test]
    fn fractional_screen_edges_cannot_invert_clamp_or_escape_screen() {
        let mut anchor = anchor_from_data(&outputs(), &panels()).unwrap();
        anchor.screen.x = 1920.5;
        anchor.screen.y = 0.5;
        anchor.top = 0.5;
        let bounds = below_panel(&anchor, 9999, 9999).unwrap();
        assert_eq!(
            bounds,
            Placement {
                x: 1921,
                y: 33,
                width: 2559,
                height: 1407
            }
        );
        anchor.screen.width = 1.;
        assert!(below_panel(&anchor, 100, 100).is_err());
    }
    #[test]
    fn anchor_rejects_invalid_geometry_and_uses_clock_fallback() {
        let mut panels = panels();
        panels[3]["widgets"][1]["width"] = json!(-1);
        let anchor = anchor_from_data(&outputs(), &panels).unwrap();
        assert_eq!(anchor.widget, 21);
        assert!(!anchor.native);
        let mut outputs = outputs();
        outputs["outputs"][0]["size"]["height"] = json!(0);
        assert!(anchor_from_data(&outputs, &panels).is_err());
        for bad in [f64::NAN, f64::INFINITY, -1., 0.] {
            assert!(Rect {
                x: 0.,
                y: 0.,
                width: bad,
                height: 32.
            }
            .validate()
            .is_err());
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn discovery_argv_is_bounded_helper_ready_without_live_queries() {
        let [outputs, panels] = discovery_commands();
        assert_eq!(outputs.get_program(), "kscreen-doctor");
        assert_eq!(outputs.get_args().collect::<Vec<_>>(), ["-j"]);
        assert_eq!(panels.get_program(), "qdbus6");
        assert_eq!(panels.get_args().count(), 4);
        assert!(DISCOVERY_SCRIPT.contains("width:r.width,height:r.height"));
    }
}
