//! Narrow one-shot KWin fallback. Generation tests never contact a compositor.
//! The caller must read back the actual window geometry after this submission;
//! a successful qdbus call alone does not prove KWin applied the requested bounds.
use super::{anchor::Placement, status_file::Identity};
use crate::{error::IslandError, identity, services::process::run_bounded};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn placement_script(pid: u32, overlay: bool, bounds: Placement) -> Result<String, IslandError> {
    if pid == 0
        || pid > i32::MAX as u32
        || bounds.width == 0
        || bounds.height == 0
        || bounds.width > 32768
        || bounds.height > 32768
        || bounds.x.unsigned_abs() > 1_000_000
        || bounds.y.unsigned_abs() > 1_000_000
    {
        return Err(IslandError::invalid(
            "Invalid KWin placement identity or geometry",
        ));
    }
    let string = |value: &str| {
        serde_json::to_string(value)
            .map_err(|_| IslandError::internal("Cannot encode window identity"))
    };
    let title = string(if overlay {
        identity::TITLE_OVERLAY
    } else {
        identity::TITLE_PREVIEW
    })?;
    let name = string(identity::WM_CLASS_INSTANCE)?;
    let class = string(identity::WM_CLASS_CLASS)?;
    Ok(format!(
        r#"// One instance, one frozen identity; no activation or global rule.
const matches = workspace.windowList().filter(w =>
    w.pid === {pid} && String(w.caption) === {title} &&
    String(w.resourceName) === {name} && String(w.resourceClass) === {class} &&
    w.notification === true);
if (matches.length !== 1) {{ throw new Error("Unique NOTIFICATION island identity not found"); }}
const w = matches[0];
w.frameGeometry = {{x:{x},y:{y},width:{width},height:{height}}};
w.keepAbove = true;
"#,
        x = bounds.x,
        y = bounds.y,
        width = bounds.width,
        height = bounds.height
    ))
}

fn call(arguments: &[&str]) -> Result<Vec<u8>, IslandError> {
    let mut command = Command::new("qdbus6");
    command.arg("org.kde.KWin").args(arguments);
    run_bounded(command, Duration::from_secs(3), 4096, false)
}
struct ScriptFile {
    path: PathBuf,
    identity: Identity,
}
impl Drop for ScriptFile {
    fn drop(&mut self) {
        if self.identity.matches(&self.path) {
            if let Err(error) = fs::remove_file(&self.path) {
                tracing::warn!(%error, "Cannot remove owned KWin script file");
            }
        }
    }
}
fn submit(directory: &Path, overlay: bool, bounds: Placement) -> Result<(), IslandError> {
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| IslandError::io("Cannot inspect KWin script directory"))?;
    // SAFETY: geteuid has no preconditions.
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(IslandError::invalid(
            "KWin script directory must be private",
        ));
    }
    let pid = std::process::id();
    let name = format!(
        "ripple-placement-{pid}-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let path = directory.join(format!("{name}.js"));
    let script = placement_script(pid, overlay, bounds)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| IslandError::io("Cannot create private KWin script"))?;
    let owned = ScriptFile {
        identity: Identity::at(&path)
            .map_err(|_| IslandError::io("Cannot record KWin script ownership"))?,
        path,
    };
    file.write_all(script.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| IslandError::io("Cannot write KWin script"))?;
    let path = owned
        .path
        .to_str()
        .ok_or_else(|| IslandError::invalid("KWin script path must be UTF-8"))?;
    // Never unload a pre-existing/global script. The exclusive file and per-call
    // name belong to this invocation; only this name is passed to unload.
    let run: Result<(), IslandError> = (|| {
        let loaded = call(&[
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
            path,
            &name,
        ])?;
        let id = std::str::from_utf8(&loaded)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
            .filter(|id| *id <= i32::MAX as u32)
            .ok_or_else(|| {
                IslandError::unavailable("KWin returned an invalid script identifier")
            })?;
        call(&[&format!("/Scripting/Script{id}"), "org.kde.kwin.Script.run"])?;
        Ok(())
    })();
    // Attempt unload even when parsing/run fails; propagate every cleanup error.
    let unload =
        call(&["/Scripting", "org.kde.kwin.Scripting.unloadScript", &name]).and_then(|bytes| {
            if std::str::from_utf8(&bytes).is_ok_and(|s| s.trim() == "true") {
                Ok(())
            } else {
                Err(IslandError::unavailable(
                    "KWin did not confirm own script unload",
                ))
            }
        });
    match (run, unload) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(_)) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(IslandError::unavailable(format!(
            "{}; own script unload also failed: {}",
            error.message, cleanup.message
        ))),
    }
}
/// Off-main-thread load/run/unload; only the current process PID can be targeted.
/// This reports submission, NOT compositor readback/placement verification.
pub async fn place(
    directory: PathBuf,
    overlay: bool,
    bounds: Placement,
) -> Result<(), IslandError> {
    tokio::task::spawn_blocking(move || submit(&directory, overlay, bounds))
        .await
        .map_err(|_| IslandError::internal("KWin placement worker failed"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kwin_script_matches_exact_pid_title_class_and_notification_only() {
        let script = placement_script(
            42,
            true,
            Placement {
                x: 210,
                y: 32,
                width: 1180,
                height: 820,
            },
        )
        .unwrap();
        for predicate in [
            "w.pid === 42",
            "String(w.caption) === \"Hermes Island — panel overlay\"",
            "String(w.resourceName) === \"hermes-island\"",
            "String(w.resourceClass) === \"Hermes-island\"",
            "w.notification === true",
            "matches.length !== 1",
            "x:210,y:32,width:1180,height:820",
        ] {
            assert!(
                script.contains(predicate),
                "missing narrow identity/placement predicate {predicate}"
            );
        }
        for forbidden in [
            "activate",
            "workspace.activeWindow",
            "windowAdded",
            "indexOf",
            "startsWith",
            "setTimeout",
        ] {
            assert!(
                !script.contains(forbidden),
                "unexpected broad/focus behavior {forbidden}"
            );
        }
        assert!(placement_script(
            42,
            false,
            Placement {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            }
        )
        .unwrap()
        .contains(identity::TITLE_PREVIEW));
        assert!(placement_script(
            0,
            true,
            Placement {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            }
        )
        .is_err());
    }
}
