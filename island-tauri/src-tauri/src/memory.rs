//! Process-wide memory knobs, applied once at the top of `run()` before any thread or
//! allocation-heavy work starts.
//!
//! - The async runtime: Tauri's default is a tokio multi-thread runtime with one worker per CPU
//!   (16 on a 16-thread desktop) and up to 512 blocking threads. The island's async work is IPC, D-Bus and
//!   timers; every blocking call goes through `spawn_blocking` (commands::blocking), so a few
//!   workers carry it. Each idle worker still costs a stack and a glibc malloc arena.
//! - glibc malloc arenas: by default up to 8 × CPUs, each keeping its own freed pages. Two
//!   arenas keep the multi-threaded allocation path while capping that retained memory.

/// tokio worker threads for `tauri::async_runtime`.
pub const ASYNC_WORKERS: usize = 4;
/// Cap on tokio's on-demand blocking pool. The audit (VERIFICATION.md, W2) found no blocking
/// task that waits on another one, and the longest concurrent holders are the input-region
/// main-thread waits (≤ 2 s each, serialised on one mutex), `ai_usage` (≤ 30 s, serialised on
/// its refresh mutex) and the run_bounded helpers (fonts, outputs, palette, voice, apps). 32
/// covers every one of them at once with room to spare; idle blocking threads exit after 10 s.
pub const MAX_BLOCKING_THREADS: usize = 32;
/// glibc `M_ARENA_MAX`.
pub const MALLOC_ARENAS: i32 = 2;

/// The runtime Tauri uses for every `async_runtime::spawn/spawn_blocking/block_on`.
pub fn async_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(ASYNC_WORKERS)
        .max_blocking_threads(MAX_BLOCKING_THREADS)
        .enable_all()
        .build()
}

/// Cap malloc arenas (glibc only; a no-op elsewhere). Must run before other threads start.
pub fn limit_malloc_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: mallopt only adjusts allocator tunables; called once, single-threaded, at start.
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, MALLOC_ARENAS);
    }
}

/// Hand the small runtime to Tauri; must run before anything touches `tauri::async_runtime`.
/// If it cannot be built, Tauri falls back to its own default one.
pub fn install_async_runtime() {
    match async_runtime() {
        Ok(runtime) => {
            // Tauri keeps only the handle; the runtime must live for the whole process.
            let runtime: &'static tokio::runtime::Runtime = Box::leak(Box::new(runtime));
            tauri::async_runtime::set(runtime.handle().clone());
        }
        Err(error) => tracing::warn!(%error, "Could not build the small async runtime; using Tauri's default"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_has_the_documented_worker_count() {
        let runtime = async_runtime().unwrap();
        assert_eq!(runtime.metrics().num_workers(), ASYNC_WORKERS);
        assert_eq!(ASYNC_WORKERS, 4);
    }

    #[test]
    fn blocking_pool_runs_more_concurrent_holders_than_workers() {
        // The commands that park a blocking slot for seconds (input-region main-thread waits,
        // ai_usage, run_bounded helpers) can overlap; they must never queue behind each other
        // just because the worker count is small.
        let runtime = async_runtime().unwrap();
        let holders = 24;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(holders));
        let tasks: Vec<_> = (0..holders)
            .map(|_| {
                let barrier = barrier.clone();
                runtime.spawn_blocking(move || {
                    barrier.wait();
                })
            })
            .collect();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                for task in tasks {
                    task.await.unwrap();
                }
            })
            .await
            .expect("blocking holders deadlocked: the pool is smaller than the concurrent holders");
        });
        assert!(MAX_BLOCKING_THREADS >= holders);
    }
}
