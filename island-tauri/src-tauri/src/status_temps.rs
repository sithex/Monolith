//! Crash leftovers from atomic status publication. Called only while BOTH instance locks
//! are held, after the ordinary status/socket ownership checks have succeeded.
use anyhow::{Context, Result};
use std::{fs, os::unix::fs::MetadataExt, path::Path};

fn owner(name: &str) -> Option<u32> {
    let rest = name.strip_prefix(".status.json.")?.strip_suffix(".tmp")?;
    let (pid, serial) = rest.split_once('.')?;
    if pid.is_empty()
        || serial.is_empty()
        || !pid.bytes().all(|b| b.is_ascii_digit())
        || !serial.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let pid: u32 = pid.parse().ok()?;
    let _: u64 = serial.parse().ok()?;
    (pid > 0 && pid <= i32::MAX as u32).then_some(pid)
}

pub(crate) fn reclaim(data: &Path, uid: u32) -> Result<()> {
    for entry in fs::read_dir(data).context("Reading status crash leftovers")? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(owner) else {
            continue;
        };
        // Only NotFound establishes death: permission/I/O failures are not evidence.
        match fs::symlink_metadata(Path::new("/proc").join(pid.to_string())) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => continue,
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.uid() != uid || meta.nlink() != 1 {
            continue;
        }
        let current = fs::symlink_metadata(&path)?;
        if current.dev() != meta.dev() || current.ino() != meta.ino() {
            continue;
        }
        fs::remove_file(&path).context("Removing dead-owner status crash leftover")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::singleton::{acquire, tests::Fixture};
    use std::{
        os::{
            fd::AsRawFd,
            unix::fs::{symlink, PermissionsExt},
        },
        process::Command,
    };
    fn dead(dir: &Path) -> u32 {
        let mut command = Command::new("/usr/bin/true");
        command.env_clear();
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
            "XDG_RUNTIME_DIR",
            "TMPDIR",
        ] {
            command.env(key, dir);
        }
        let mut p = command.spawn().unwrap();
        let id = p.id();
        p.wait().unwrap();
        id
    }
    #[test]
    fn dead_pid_status_temp_is_reclaimed_under_both_locks() {
        let f = Fixture::new();
        let path = f.0.join(format!(".status.json.{}.17.tmp", dead(&f.0)));
        fs::write(&path, "partial").unwrap();
        let _guard = acquire(&f.0).unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn live_pid_status_temp_is_never_reclaimed() {
        let f = Fixture::new();
        let path =
            f.0.join(format!(".status.json.{}.17.tmp", std::process::id()));
        fs::write(&path, "live").unwrap();
        let before = fs::metadata(&path).unwrap().ino();
        let _guard = acquire(&f.0).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().ino(), before);
        assert_eq!(fs::read_to_string(path).unwrap(), "live");
    }
    #[test]
    fn either_held_lock_prevents_temp_reclamation() {
        for name in ["island.lock", "instance.lock"] {
            let f = Fixture::new();
            let path = f.0.join(format!(".status.json.{}.0.tmp", dead(&f.0)));
            fs::write(&path, "untouched").unwrap();
            let lock_path = f.0.join(name);
            fs::write(&lock_path, "").unwrap();
            fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600)).unwrap();
            let lock = fs::File::open(lock_path).unwrap();
            assert_eq!(
                unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
            assert!(acquire(&f.0).is_err());
            assert_eq!(fs::read_to_string(path).unwrap(), "untouched");
        }
    }
    #[test]
    fn unsafe_and_malformed_status_temp_names_are_preserved() {
        let f = Fixture::new();
        let pid = dead(&f.0);
        for name in [
            ".status.json.0.1.tmp".into(),
            format!(".status.json.{pid}.bad.tmp"),
            format!(".status.json.{pid}.1.tmp.extra"),
        ] {
            fs::write(f.0.join(name), "kept").unwrap();
        }
        let target = f.0.join("target");
        fs::write(&target, "target").unwrap();
        let link = f.0.join(format!(".status.json.{pid}.1.tmp"));
        symlink(&target, &link).unwrap();
        let hard = f.0.join(format!(".status.json.{pid}.2.tmp"));
        fs::hard_link(&target, &hard).unwrap();
        let directory = f.0.join(format!(".status.json.{pid}.3.tmp"));
        fs::create_dir(&directory).unwrap();
        let _guard = acquire(&f.0).unwrap();
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 9);
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(hard).unwrap(), "target");
        assert!(directory.is_dir());
    }
}
