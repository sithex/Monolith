//! One read-only SQLite open shared by every reader of Hermes' `state.db` (AI Agents Live and
//! the agent chips). Never writes, VACUUMs, checkpoints or creates the database or its WAL/SHM.
//! Refuses a missing or non-regular file and never follows a symlink.

use rusqlite::{ffi, Connection, OpenFlags};
use std::{
    ffi::{c_char, c_int},
    fs, io,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
    sync::OnceLock,
    time::Duration,
};

/// Longest single text/blob value the connection will materialize.
pub const MAX_VALUE_BYTES: c_int = 4 * 1024 * 1024;

/// First line of a SQLite error, bounded for logs and the UI.
pub(crate) fn err_str(e: &rusqlite::Error) -> String {
    let s = e.to_string();
    let first = s.lines().next().unwrap_or_default();
    first.chars().take(160).collect()
}

/// Opens without following a symlink and without blocking on a FIFO; refuses anything but a
/// regular file.
pub(crate) fn open_regular(path: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

/// Read-only connection: private read-only VFS, `mode=ro&readonly_shm=1`, `NOFOLLOW`, the given
/// busy timeout, a 4 MiB value limit, `query_only=ON` and `trusted_schema=OFF`. Errors are
/// "state.db not found" / "state.db unreadable" before SQLite runs, else SQLite's first line.
pub fn open(path: &Path, busy_timeout: Duration) -> Result<Connection, String> {
    open_regular(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            "state.db not found"
        } else {
            "state.db unreadable"
        }
        .to_owned()
    })?;
    register_ro_vfs()?;
    let db = Connection::open_with_flags_and_vfs(
        ro_uri(path)?,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        RO_VFS_NAME,
    )
    .map_err(|e| err_str(&e))?;
    db.busy_timeout(busy_timeout).map_err(|e| err_str(&e))?;
    // SAFETY: the handle belongs to this live connection. This connection-local limit
    // bounds a corrupt/unexpected text/blob row without changing the source database.
    unsafe {
        ffi::sqlite3_limit(db.handle(), ffi::SQLITE_LIMIT_LENGTH, MAX_VALUE_BYTES);
    }
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")
        .map_err(|e| err_str(&e))?;
    Ok(db)
}

/// Escape original path bytes rather than a lossy UTF-8 conversion. Absolute paths
/// cannot accidentally become a URI authority.
pub(crate) fn ro_uri(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "state.db path unavailable".to_owned())?
            .join(path)
    };
    let mut p = String::new();
    for &byte in absolute.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            p.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(p, "%{byte:02X}");
        }
    }
    // `mode=ro` alone still lets SQLite CREATE/open a WAL read-write and modify/create
    // its SHM index. Never use immutable=1: it silently ignores committed WAL frames.
    Ok(format!("file:{p}?mode=ro&readonly_shm=1"))
}

const RO_VFS_NAME: &str = "agent-live-read-only";
static BASE_VFS: OnceLock<usize> = OnceLock::new();
static BASE_METHODS: OnceLock<ffi::sqlite3_io_methods> = OnceLock::new();
static RO_METHODS: OnceLock<ffi::sqlite3_io_methods> = OnceLock::new();

/// SQLite opens WAL files with READWRITE|CREATE even for mode=ro databases. This private
/// VFS delegates to bundled SQLite's Unix VFS but forces every named open read-only,
/// denies temporary files/deletions, and leaves SHM to readonly_shm=1. Registration
/// lasts for the process lifetime and does not change SQLite's default VFS.
fn register_ro_vfs() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            // SAFETY: SQLite owns the native VFS for process lifetime. Registration is
            // serialized by OnceLock; the copied structure is leaked for that lifetime.
            // Unchanged function pointers/app data still refer to bundled SQLite's Unix VFS.
            unsafe {
                let base = ffi::sqlite3_vfs_find(c"unix".as_ptr());
                if base.is_null() || (*base).xOpen.is_none() {
                    return Err("read-only SQLite VFS unavailable".to_owned());
                }
                let _ = BASE_VFS.set(base as usize);
                let mut vfs = Box::new(std::ptr::read(base));
                vfs.zName = c"agent-live-read-only".as_ptr();
                vfs.pNext = std::ptr::null_mut();
                vfs.xOpen = Some(ro_vfs_open);
                vfs.xDelete = Some(ro_vfs_delete);
                let vfs = Box::into_raw(vfs);
                if ffi::sqlite3_vfs_register(vfs, 0) != ffi::SQLITE_OK {
                    drop(Box::from_raw(vfs));
                    return Err("read-only SQLite VFS registration failed".to_owned());
                }
                Ok(())
            }
        })
        .clone()
}

unsafe extern "C" fn ro_vfs_open(
    _vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    if name.is_null() || flags & (ffi::SQLITE_OPEN_MAIN_DB | ffi::SQLITE_OPEN_WAL) == 0 {
        return ffi::SQLITE_READONLY;
    }
    let Some(&base) = BASE_VFS.get() else {
        return ffi::SQLITE_CANTOPEN;
    };
    // SAFETY: SQLite supplies valid file/name/out_flags arguments for xOpen. BASE_VFS
    // is set before this callback is registered and the native VFS never expires.
    unsafe {
        let base = base as *mut ffi::sqlite3_vfs;
        let Some(open) = (*base).xOpen else {
            return ffi::SQLITE_CANTOPEN;
        };
        let flags = (flags
            & !(ffi::SQLITE_OPEN_READWRITE
                | ffi::SQLITE_OPEN_CREATE
                | ffi::SQLITE_OPEN_DELETEONCLOSE))
            | ffi::SQLITE_OPEN_READONLY
            | ffi::SQLITE_OPEN_NOFOLLOW;
        let rc = open(base, name, file, flags, out_flags);
        if rc == ffi::SQLITE_OK && flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
            // The Unix VFS can reuse an SHM mapping opened by another connection in
            // this process. Tell SQLite it is read-only even if that mapping is RW,
            // so this reader never updates read marks or unlinks the shared index.
            let original = BASE_METHODS.get_or_init(|| *(*file).pMethods);
            let methods = RO_METHODS.get_or_init(|| {
                let mut methods = *original;
                methods.xShmMap = Some(ro_shm_map);
                methods.xShmUnmap = Some(ro_shm_unmap);
                methods
            });
            (*file).pMethods = methods;
        }
        rc
    }
}

unsafe extern "C" fn ro_shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    size: c_int,
    _extend: c_int,
    out: *mut *mut std::ffi::c_void,
) -> c_int {
    let Some(map) = BASE_METHODS.get().and_then(|m| m.xShmMap) else {
        return ffi::SQLITE_READONLY;
    };
    // SAFETY: arguments are the native SQLite xShmMap arguments. Never extend the
    // existing index; a successful mapping is advertised as read-only to the pager.
    let rc = unsafe { map(file, page, size, 0, out) };
    if rc == ffi::SQLITE_OK {
        ffi::SQLITE_READONLY
    } else {
        rc
    }
}

unsafe extern "C" fn ro_shm_unmap(file: *mut ffi::sqlite3_file, _delete: c_int) -> c_int {
    let Some(unmap) = BASE_METHODS.get().and_then(|m| m.xShmUnmap) else {
        return ffi::SQLITE_OK;
    };
    // SAFETY: the native file object is unchanged, and deletion is always disabled.
    unsafe { unmap(file, 0) }
}

unsafe extern "C" fn ro_vfs_delete(
    _vfs: *mut ffi::sqlite3_vfs,
    _name: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    ffi::SQLITE_READONLY
}
