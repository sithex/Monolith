//! Monitor → Processes: a `/proc` process table with CPU, private memory and per-process GPU
//! usage, plus a guarded kill. Read only when the Processes tab polls; nothing runs in the
//! background. GPU numbers come from the DRM fdinfo interface (`drm-engine-*` busy time and
//! `drm-memory-vram`), which amdgpu, i915 and xe expose for every open render client.

use crate::contract::system::{KillSignal, ProcessList, ProcessRow};
use crate::error::IslandError;
use std::{
    collections::{HashMap, HashSet},
    fs,
    time::Instant,
};

/// Fields of `/proc/<pid>/stat` the table needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub comm: String,
    pub ppid: u32,
    pub cpu_ticks: u64,
    pub threads: u32,
    pub start_ticks: u64,
}

/// `comm` may contain spaces and parentheses, so split at the LAST `)`.
pub fn parse_stat(text: &str) -> Option<Stat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_owned();
    // Fields after `)`: [0]=state [1]=ppid … [11]=utime [12]=stime … [17]=num_threads [19]=starttime
    let rest: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    let num = |i: usize| rest.get(i)?.parse::<u64>().ok();
    Some(Stat {
        comm,
        ppid: u32::try_from(num(1)?).ok()?,
        cpu_ticks: num(11)?.saturating_add(num(12)?),
        threads: u32::try_from(num(17)?).unwrap_or(u32::MAX),
        start_ticks: num(19)?,
    })
}

/// `/proc/<pid>/statm`: (resident − shared) pages = anonymous resident memory.
pub fn parse_statm_private(text: &str) -> Option<u64> {
    let mut it = text.split_whitespace().skip(1);
    let resident = it.next()?.parse::<u64>().ok()?;
    let shared = it.next()?.parse::<u64>().ok()?;
    Some(resident.saturating_sub(shared))
}

/// One DRM client as seen through an fd's fdinfo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrmClient {
    /// `drm-pdev` + `drm-client-id`: unique per open client across the whole system.
    pub key: String,
    /// Busy time of the render-ish engines (gfx, compute, render, …) in ns.
    pub engine_ns: u64,
    pub vram_bytes: u64,
}

/// Parse a DRM fdinfo. Returns `None` for non-DRM fds or clients without a client id.
pub fn parse_drm_fdinfo(text: &str) -> Option<DrmClient> {
    let mut client = None::<String>;
    let mut pdev = String::new();
    let mut out = DrmClient::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "drm-client-id" => client = Some(value.to_owned()),
            "drm-pdev" => pdev = value.to_owned(),
            k if k.starts_with("drm-engine-") && !k.starts_with("drm-engine-capacity-") => {
                // amdgpu: gfx/compute/dma/dec/enc; i915/xe: render/copy/video/compute.
                let engine = &k["drm-engine-".len()..];
                if matches!(engine, "gfx" | "compute" | "render" | "rcs" | "ccs") {
                    let ns = value
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u64>().ok())
                        .unwrap_or(0);
                    out.engine_ns = out.engine_ns.saturating_add(ns);
                }
            }
            "drm-memory-vram" | "drm-resident-vram0" | "drm-total-vram" => {
                let mut parts = value.split_whitespace();
                let n = parts.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
                let unit = match parts.next() {
                    Some("KiB") => 1024,
                    Some("MiB") => 1024 * 1024,
                    Some("GiB") => 1024 * 1024 * 1024,
                    _ => 1,
                };
                // Keep the largest reading when a driver reports more than one key.
                out.vram_bytes = out.vram_bytes.max(n.saturating_mul(unit));
            }
            _ => {}
        }
    }
    out.key = format!("{pdev}/{}", client?);
    Some(out)
}

/// Previous counters, keyed by (pid, start time) so a recycled pid starts fresh.
#[derive(Default)]
pub struct Sampler {
    last: Option<Instant>,
    cpu: HashMap<(u32, u64), u64>,
    gpu: HashMap<String, u64>,
}

struct Raw {
    pid: u32,
    stat: Stat,
    command: String,
    name: String,
    private_pages: u64,
    own: bool,
    drm: Vec<DrmClient>,
}

impl Sampler {
    pub fn sample(&mut self) -> ProcessList {
        let now = Instant::now();
        let elapsed = self.last.map(|t| now.duration_since(t).as_secs_f64());
        self.last = Some(now);
        let uid = unsafe { libc::getuid() };
        let me = std::process::id();
        let mut raws = Vec::new();
        let mut seen_clients = HashSet::new();
        if let Ok(dir) = fs::read_dir("/proc") {
            for entry in dir.flatten() {
                let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok())
                else {
                    continue;
                };
                if let Some(raw) = read_process(pid, uid, me, &mut seen_clients) {
                    raws.push(raw);
                }
            }
        }
        raws.sort_by_key(|r| r.pid);
        let ticks = super::self_cost::ticks_per_second();
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        let page = page_size() as f64;
        let mut cpu_next = HashMap::with_capacity(raws.len());
        let mut gpu_next = HashMap::new();
        let mut gpu_available = false;
        let rows = raws
            .into_iter()
            .map(|r| {
                let key = (r.pid, r.stat.start_ticks);
                let cpu = match (elapsed, self.cpu.get(&key)) {
                    (Some(secs), Some(&before)) if secs > 0.05 => Some(
                        (r.stat.cpu_ticks.saturating_sub(before) as f64 / ticks / secs / cpus
                            * 100.0)
                            .clamp(0.0, 100.0),
                    ),
                    _ => None,
                };
                cpu_next.insert(key, r.stat.cpu_ticks);
                let (mut gpu, mut vram) = (None::<f64>, None::<f64>);
                for client in &r.drm {
                    gpu_available = true;
                    *vram.get_or_insert(0.0) += client.vram_bytes as f64;
                    if let (Some(secs), Some(&before)) = (elapsed, self.gpu.get(&client.key)) {
                        if secs > 0.05 {
                            let pct = client.engine_ns.saturating_sub(before) as f64
                                / (secs * 1e9)
                                * 100.0;
                            *gpu.get_or_insert(0.0) += pct;
                        }
                    }
                    gpu_next.insert(client.key.clone(), client.engine_ns);
                }
                ProcessRow {
                    pid: r.pid,
                    ppid: r.stat.ppid,
                    start_ticks: r.stat.start_ticks as f64,
                    name: r.name,
                    command: r.command,
                    cpu,
                    memory_bytes: r.private_pages as f64 * page,
                    gpu: gpu.map(|g| g.clamp(0.0, 100.0)),
                    vram_bytes: vram,
                    threads: r.stat.threads,
                    own: r.own,
                }
            })
            .collect();
        self.cpu = cpu_next;
        self.gpu = gpu_next;
        ProcessList {
            rows,
            gpu_available,
            memory_total: memory_total().unwrap_or(0.0),
            cpu_count: cpus as u32,
        }
    }
}

fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions.
    let n = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if n > 0 {
        n as u64
    } else {
        4096
    }
}

fn memory_total() -> Option<f64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<f64>().ok()?;
    Some(kib * 1024.0)
}

fn owner(pid: u32) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(format!("/proc/{pid}")).ok().map(|m| m.uid())
}

/// A friendlier name than the 15-char `comm`: the argv[0] basename when it extends comm
/// (e.g. `WebKitWebProces` → `WebKitWebProcess`), otherwise comm.
pub fn display_name(comm: &str, argv0: &str) -> String {
    let base = argv0.rsplit('/').next().unwrap_or(argv0);
    if !base.is_empty() && base.len() > comm.len() && base.starts_with(comm) {
        base.to_owned()
    } else {
        comm.to_owned()
    }
}

fn read_process(
    pid: u32,
    uid: u32,
    me: u32,
    seen_clients: &mut HashSet<String>,
) -> Option<Raw> {
    let stat = parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    // Kernel threads have no command line; they are not something to manage from here.
    if cmdline.is_empty() {
        return None;
    }
    let args: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let mut command = args.join(" ");
    if command.len() > 400 {
        let mut cut = 400;
        while !command.is_char_boundary(cut) {
            cut -= 1;
        }
        command.truncate(cut);
        command.push('…');
    }
    let name = display_name(&stat.comm, args.first().map(String::as_str).unwrap_or(""));
    let private_pages = fs::read_to_string(format!("/proc/{pid}/statm"))
        .ok()
        .and_then(|t| parse_statm_private(&t))
        .unwrap_or(0);
    let own = owner(pid) == Some(uid) && pid != me;
    let mut drm = Vec::new();
    // Other users' fd tables are unreadable (EACCES), so their GPU use stays unknown.
    if let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) {
        for fd in fds.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else {
                continue;
            };
            if !target.starts_with("/dev/dri/") {
                continue;
            }
            let info = format!("/proc/{pid}/fdinfo/{}", fd.file_name().to_string_lossy());
            let Some(client) = fs::read_to_string(info)
                .ok()
                .and_then(|t| parse_drm_fdinfo(&t))
            else {
                continue;
            };
            // A client shared by fork/dup is counted once, at the first (lowest) pid.
            if seen_clients.insert(client.key.clone()) {
                drm.push(client);
            }
        }
    }
    Some(Raw {
        pid,
        stat,
        command,
        name,
        private_pages,
        own,
        drm,
    })
}

/// Signal one process. Refuses pid ≤ 1, the island itself, other users' processes and a
/// pid whose start time no longer matches (it exited and the number was reused).
pub fn kill(pid: u32, start_ticks: f64, signal: KillSignal) -> Result<(), IslandError> {
    if pid <= 1 || pid == std::process::id() {
        return Err(IslandError::invalid("that process cannot be ended from here"));
    }
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|t| parse_stat(&t))
        .ok_or_else(|| IslandError::unavailable("the process has already exited"))?;
    if stat.start_ticks as f64 != start_ticks {
        return Err(IslandError::unavailable("the process has already exited"));
    }
    if owner(pid) != Some(unsafe { libc::getuid() }) {
        return Err(IslandError::invalid(
            "that process belongs to another user (system processes need root)",
        ));
    }
    let sig = match signal {
        KillSignal::Term => libc::SIGTERM,
        KillSignal::Kill => libc::SIGKILL,
    };
    let pid = libc::pid_t::try_from(pid).map_err(|_| IslandError::invalid("bad pid"))?;
    // SAFETY: plain kill(2) on a positive, validated pid.
    if unsafe { libc::kill(pid, sig) } == 0 {
        Ok(())
    } else {
        Err(IslandError::io(format!(
            "could not signal the process: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_parses_comm_with_spaces_and_parens() {
        let text = "4242 (Web Content (x)) S 1000 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 37 0 123456 1000 2000 18446744073709551615";
        let s = parse_stat(text).unwrap();
        assert_eq!(s.comm, "Web Content (x)");
        assert_eq!(s.ppid, 1000);
        assert_eq!(s.cpu_ticks, 300);
        assert_eq!(s.threads, 37);
        assert_eq!(s.start_ticks, 123456);
    }

    #[test]
    fn statm_private_is_resident_minus_shared() {
        assert_eq!(parse_statm_private("1000 400 150 10 0 300 0"), Some(250));
        assert_eq!(parse_statm_private("1000 100 150 10 0 300 0"), Some(0));
        assert_eq!(parse_statm_private("garbage"), None);
    }

    #[test]
    fn amdgpu_fdinfo_sums_render_engines_and_vram() {
        let text = "pos:\t0\nflags:\t02100002\ndrm-driver:\tamdgpu\ndrm-client-id:\t58431\ndrm-pdev:\t0000:07:00.0\n\
drm-engine-compute:\t892 ns\ndrm-engine-gfx:\t1000 ns\ndrm-engine-dma:\t5000 ns\ndrm-memory-vram:\t670624 KiB\n";
        let c = parse_drm_fdinfo(text).unwrap();
        assert_eq!(c.key, "0000:07:00.0/58431");
        assert_eq!(c.engine_ns, 1892, "dma/video engines are not GPU load");
        assert_eq!(c.vram_bytes, 670624 * 1024);
        assert!(parse_drm_fdinfo("pos:\t0\nflags:\t0\n").is_none());
    }

    #[test]
    fn display_name_extends_truncated_comm_only() {
        assert_eq!(
            display_name("WebKitWebProces", "/usr/lib/webkit/WebKitWebProcess"),
            "WebKitWebProcess"
        );
        assert_eq!(display_name("python3", "/usr/bin/python3"), "python3");
        assert_eq!(display_name("bash", "/bin/sh"), "bash");
    }

    #[test]
    fn live_table_has_self_and_cpu_on_second_sample() {
        let mut s = Sampler::default();
        let first = s.sample();
        let me = std::process::id();
        let row = first.rows.iter().find(|r| r.pid == me).expect("own process listed");
        assert!(row.cpu.is_none(), "first interval is unavailable");
        assert!(!row.own, "the island itself is never offered for killing");
        assert!(first.memory_total > 0.0);
        std::thread::sleep(std::time::Duration::from_millis(80));
        let second = s.sample();
        assert!(second.rows.iter().find(|r| r.pid == me).unwrap().cpu.is_some());
    }

    #[test]
    fn kill_refuses_self_init_and_stale_start_time() {
        assert!(kill(1, 0.0, KillSignal::Term).is_err());
        assert!(kill(std::process::id(), 0.0, KillSignal::Term).is_err());
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let stat = parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).unwrap()).unwrap();
        assert!(kill(pid, stat.start_ticks as f64 + 1.0, KillSignal::Term).is_err());
        assert!(child.try_wait().unwrap().is_none(), "stale start time must not signal");
        kill(pid, stat.start_ticks as f64, KillSignal::Kill).unwrap();
        let status = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }
}
