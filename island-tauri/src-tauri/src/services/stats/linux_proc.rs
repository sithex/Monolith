//! Native Linux counters (port of island-rust `stats.rs`); the first CPU/network interval is
//! unavailable, not zero.

use crate::contract::system::{DiskStats, NetworkStats, SystemStats};
use std::{collections::BTreeMap, fs, path::Path, time::Instant};

type Interfaces = BTreeMap<String, (u64, u64)>;

#[derive(Default)]
pub struct Sampler {
    cpu: Option<(u64, u64)>,
    network: Option<(Interfaces, Instant)>,
}

impl Sampler {
    pub fn sample(&mut self, disk: &Path) -> SystemStats {
        let cpu = fs::read_to_string("/proc/stat").unwrap_or_default();
        let mem = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let net = fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let mut result = self.sample_inputs(&cpu, &mem, &net, Instant::now());
        result.hostname = fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|s| s.trim().to_owned())
            .ok();
        result.cpu_count = std::thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok());
        result.uptime = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map(f64::round);
        result.load_average = fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .take(3)
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
            })
            .filter(|v| v.len() == 3 && v.iter().all(|n| n.is_finite()))
            .and_then(|v| <[f64; 3]>::try_from(v).ok());
        result.disk = disk_stats(disk);
        result
    }

    pub(crate) fn sample_inputs(
        &mut self,
        cpu: &str,
        mem: &str,
        net: &str,
        now: Instant,
    ) -> SystemStats {
        let mut result = SystemStats {
            cpu: None,
            mem_pct: None,
            mem_used: None,
            mem_total: None,
            uptime: None,
            load_average: None,
            cpu_count: None,
            hostname: None,
            platform: "linux".to_owned(),
            disk: None,
            network: None,
        };
        let current = cpu_counters(cpu);
        if let (Some((old_total, old_idle)), Some((total, idle))) = (self.cpu, current) {
            if total > old_total && idle >= old_idle {
                result.cpu = Some(round_tenth(
                    ((1.0 - (idle - old_idle) as f64 / (total - old_total) as f64) * 100.0)
                        .clamp(0.0, 100.0),
                ));
            }
        }
        self.cpu = current;
        let memory: BTreeMap<_, _> = mem
            .lines()
            .filter_map(|line| {
                let (key, rest) = line.split_once(':')?;
                Some((
                    key,
                    rest.split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()?
                        .checked_mul(1024)?,
                ))
            })
            .collect();
        if let (Some(&total), Some(&available)) =
            (memory.get("MemTotal"), memory.get("MemAvailable"))
        {
            if total > 0 && available <= total {
                result.mem_total = Some(total as f64);
                result.mem_used = Some((total - available) as f64);
                result.mem_pct = Some(round_tenth(
                    (total - available) as f64 / total as f64 * 100.0,
                ));
            }
        }
        if net.is_empty() {
            self.network = None;
            return result;
        }
        let interfaces: Interfaces = net
            .lines()
            .filter_map(|line| {
                let (name, rest) = line.split_once(':')?;
                let name = name.trim();
                if name == "lo" {
                    return None;
                }
                let values = rest
                    .split_whitespace()
                    .map(str::parse::<u64>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                if values.len() < 16 {
                    return None;
                }
                Some((name.to_owned(), (values[0], values[8])))
            })
            .collect();
        if let Some((before, timestamp)) = &self.network {
            if now > *timestamp && !interfaces.is_empty() && interfaces.keys().eq(before.keys()) {
                let delta = interfaces
                    .iter()
                    .try_fold((0u64, 0u64), |(rx, tx), (name, (a, b))| {
                        let (x, y) = before[name];
                        Some((
                            rx.checked_add(a.checked_sub(x)?)?,
                            tx.checked_add(b.checked_sub(y)?)?,
                        ))
                    });
                if let Some((rx, tx)) = delta {
                    let seconds = now.duration_since(*timestamp).as_secs_f64();
                    result.network = Some(NetworkStats {
                        rx_bytes_per_second: rx as f64 / seconds,
                        tx_bytes_per_second: tx as f64 / seconds,
                        interfaces: interfaces.keys().cloned().collect(),
                    });
                }
            }
        }
        self.network = Some((interfaces, now));
        result
    }
}

fn round_tenth(n: f64) -> f64 {
    (n * 10.0).round() / 10.0
}

fn cpu_counters(text: &str) -> Option<(u64, u64)> {
    let mut fields = text.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values = fields
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 5 {
        return None;
    }
    Some((
        values
            .iter()
            .try_fold(0u64, |total, n| total.checked_add(*n))?,
        values[3].checked_add(values[4])?,
    ))
}

fn disk_stats(path: &Path) -> Option<DiskStats> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let cpath = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs initializes the supplied struct on success; path is NUL terminated.
    let disk = unsafe {
        let mut data = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if libc::statvfs(cpath.as_ptr(), data.as_mut_ptr()) != 0 {
            return None;
        }
        data.assume_init()
    };
    let block = disk.f_frsize as u128;
    let total = disk.f_blocks as u128 * block;
    if disk.f_bfree > disk.f_blocks {
        return None;
    }
    let used = (disk.f_blocks - disk.f_bfree) as u128 * block;
    let available = disk.f_bavail as u128 * block;
    if total == 0
        || used + available == 0
        || total > u64::MAX as u128
        || available > u64::MAX as u128
    {
        return None;
    }
    Some(DiskStats {
        total: total as f64,
        used: used as f64,
        available: available as f64,
        percent: round_tenth(used as f64 / (used + available) as f64 * 100.0),
        path: path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cpu_memory_network_contract() {
        let mut s = Sampler::default();
        let now = Instant::now();
        let mem = "MemTotal: 1000 kB\nMemAvailable: 250 kB\n";
        let before = s.sample_inputs(
            "cpu 100 0 50 800 50 0 0 0 1000 0",
            mem,
            "lo: 100 0 0 0 0 0 0 0 100 0 0 0 0 0 0 0\n eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0",
            now,
        );
        assert!(before.cpu.is_none());
        assert!(before.network.is_none());
        assert_eq!(before.mem_total, Some(1024000.0));
        assert_eq!(before.mem_used, Some(768000.0));
        assert_eq!(before.mem_pct, Some(75.0));
        let after = s.sample_inputs(
            "cpu 150 0 50 850 50 0 0 0 9000 0",
            mem,
            " eth0: 1300 0 0 0 0 0 0 0 2600 0 0 0 0 0 0 0",
            now + Duration::from_secs(2),
        );
        assert_eq!(after.cpu, Some(50.0));
        let network = after.network.unwrap();
        assert_eq!(network.rx_bytes_per_second, 150.0);
        assert_eq!(network.tx_bytes_per_second, 300.0);
        assert_eq!(network.interfaces, ["eth0"]);
        let reset = s.sample_inputs(
            "bad",
            "MemTotal: 1 kB\nMemAvailable: 2 kB",
            " eth0: 1 0 0 0 0 0 0 0 2 0 0 0 0 0 0 0",
            now + Duration::from_secs(3),
        );
        assert!(reset.cpu.is_none());
        assert!(reset.mem_pct.is_none());
        assert!(reset.network.is_none());
    }

    #[test]
    fn nulls_survive_serialization() {
        let mut s = Sampler::default();
        let stats = s.sample_inputs("", "", "", Instant::now());
        let wire = serde_json::to_value(&stats).unwrap();
        for key in [
            "cpu",
            "memPct",
            "memUsed",
            "memTotal",
            "uptime",
            "loadAverage",
            "cpuCount",
            "hostname",
            "disk",
            "network",
        ] {
            assert!(wire[key].is_null(), "{key} must stay null");
        }
        assert_eq!(wire["platform"], "linux");
    }
}
