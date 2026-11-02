//! System metrics (SPEC §3.5). Linux reads `/proc` + `statvfs` with no subprocess; other
//! platforms answer `unsupported` until their phase (`sysinfo`, Phase 8).

use crate::{contract::system::SystemStats, error::IslandError};
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::Mutex;

#[cfg(target_os = "linux")]
pub mod linux_proc;

pub struct StatsService {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    disk_path: PathBuf,
    #[cfg(target_os = "linux")]
    sampler: Mutex<linux_proc::Sampler>,
}

impl StatsService {
    /// `disk_path` is the filesystem whose usage is reported (the user's home).
    pub fn new(disk_path: PathBuf) -> Self {
        Self {
            disk_path,
            #[cfg(target_os = "linux")]
            sampler: Mutex::new(linux_proc::Sampler::default()),
        }
    }

    /// The first CPU/network interval is unavailable (`null`), not zero.
    #[cfg(target_os = "linux")]
    pub fn sample(&self) -> Result<SystemStats, IslandError> {
        let mut sampler = self
            .sampler
            .lock()
            .map_err(|_| IslandError::internal("stats sampler is poisoned"))?;
        Ok(sampler.sample(&self.disk_path))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn sample(&self) -> Result<SystemStats, IslandError> {
        Err(IslandError::unsupported(
            "system stats are not implemented on this platform yet",
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn native_stats_dispatch_and_nulls_first_interval() {
        let svc = StatsService::new(std::env::temp_dir());
        let first = svc.sample().unwrap();
        assert_eq!(first.platform, "linux");
        assert!(
            first.cpu.is_none(),
            "first CPU interval is unavailable, not zero"
        );
        assert!(first.mem_total.unwrap() > 0.0);
        assert!(first.disk.as_ref().unwrap().total > 0.0);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let next = svc.sample().unwrap();
        assert!(next.cpu.is_some());
        assert!(next.uptime.is_some());
        assert_eq!(next.load_average.unwrap().len(), 3);
        // Wire shape: camelCase keys exactly as the legacy renderer reads them.
        let wire = serde_json::to_value(&next).unwrap();
        for key in [
            "cpu",
            "memPct",
            "memUsed",
            "memTotal",
            "uptime",
            "loadAverage",
            "cpuCount",
            "hostname",
            "platform",
            "disk",
            "network",
        ] {
            assert!(wire.get(key).is_some(), "missing {key}");
        }
        assert_eq!(wire.as_object().unwrap().len(), 11);
        assert!(wire["disk"]["percent"].is_number());
        // An unreadable path degrades to null, never an error or zeros.
        let missing = StatsService::new("/definitely/not/a/path".into())
            .sample()
            .unwrap();
        assert!(missing.disk.is_none());
    }
}
