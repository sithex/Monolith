//! Cancellable nonblocking AF_UNIX connect. `connect_to_env()` uses a blocking
//! UnixStream connect; a wedged compositor listen backlog must not prevent join.
use super::*;
use std::{
    os::{fd::FromRawFd, unix::ffi::OsStrExt},
    path::PathBuf,
};

pub(super) fn connect(
    shared: &Shared,
    cancel: &mut UnixStream,
    stop: &AtomicBool,
) -> Result<Connection, IslandError> {
    // Do not steal an inherited WAYLAND_SOCKET belonging to a GUI toolkit. This
    // service always connects a separate socket to the compositor's named endpoint.
    let display = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    let display = PathBuf::from(display);
    let path = if display.is_absolute() {
        display
    } else {
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
            IslandError::unavailable(
                "Wayland runtime directory unavailable; native X11 clipboard is unsupported",
            )
        })?)
        .join(display)
    };
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: an all-zero sockaddr_un is valid initialized storage. sun_path is
    // populated with checked bytes, retaining a terminating zero.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(IslandError::invalid("Invalid Wayland socket path"));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, src) in address.sun_path.iter_mut().zip(bytes) {
        *dst = *src as libc::c_char;
    }
    // SAFETY: socket has no pointer arguments; ownership is established immediately.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(IslandError::io("Cannot create Wayland clipboard socket"));
    }
    // SAFETY: raw is a successful fresh socket descriptor and is now uniquely owned.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        if stop.load(Ordering::Acquire) || !shared.enabled.load(Ordering::Acquire) {
            return Err(IslandError::unavailable("Clipboard stopped"));
        }
        if Instant::now() >= deadline {
            return Err(IslandError::timeout("Wayland clipboard connect timed out"));
        }
        // This counter corresponds to an actual syscall, never a guessed test event.
        shared.status_change(|s| s.connection_attempts = s.connection_attempts.saturating_add(1));
        // SAFETY: address is initialized and sized correctly; socket is live.
        let result = unsafe {
            libc::connect(
                socket.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as libc::socklen_t,
            )
        };
        if result == 0 {
            break;
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EISCONN) => break,
            Some(libc::EINPROGRESS) | Some(libc::EALREADY) => {
                let mut fds = [
                    pfd(cancel.as_raw_fd(), libc::POLLIN),
                    pfd(socket.as_raw_fd(), libc::POLLOUT),
                ];
                poll(&mut fds, deadline.saturating_duration_since(Instant::now()))?;
                if fds[0].revents != 0 {
                    drain(cancel);
                    continue;
                }
                if fds[1].revents != 0 {
                    let mut error = 0i32;
                    let mut length = std::mem::size_of_val(&error) as libc::socklen_t;
                    // SAFETY: output pointers reference live initialized, correctly sized storage.
                    if unsafe {
                        libc::getsockopt(
                            socket.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_ERROR,
                            (&mut error as *mut i32).cast(),
                            &mut length,
                        )
                    } < 0
                        || error != 0
                    {
                        return Err(IslandError::unavailable(
                            "Wayland clipboard connection refused",
                        ));
                    }
                    break;
                }
            }
            Some(libc::EAGAIN) => {
                // Linux AF_UNIX reports a full listen backlog as EAGAIN, not an
                // in-progress connect. Retry with a deadline and cancellation FD.
                let mut fds = [pfd(cancel.as_raw_fd(), libc::POLLIN)];
                poll(
                    &mut fds,
                    Duration::from_millis(50)
                        .min(deadline.saturating_duration_since(Instant::now())),
                )?;
                if fds[0].revents != 0 {
                    drain(cancel);
                }
            }
            Some(libc::EINTR) => {}
            _ => {
                return Err(IslandError::unavailable(
                    "Cannot connect to Wayland clipboard",
                ))
            }
        }
        if Instant::now() >= deadline {
            return Err(IslandError::timeout("Wayland clipboard connect timed out"));
        }
    }
    Connection::from_socket(UnixStream::from(socket))
        .map_err(|_| IslandError::unavailable("Cannot initialize Wayland clipboard connection"))
}
