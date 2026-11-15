//! A virtual mouse on `/dev/uinput`: the only way a Wayland client can click for the user
//! without a root daemon. On a desktop session logind grants the seat user write access
//! (`uaccess`), so no group or udev rule is needed there. The device has buttons and
//! relative axes (it never moves) so libinput treats it as a pointer; the compositor clicks
//! wherever the real pointer is.
use crate::contract::apps::ClickButton;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    path::Path,
};

const PATH: &str = "/dev/uinput";
const NAME: &[u8] = b"Monolith autoclicker";

// <linux/uinput.h>
const UI_SET_EVBIT: u64 = 0x4004_5564;
const UI_SET_KEYBIT: u64 = 0x4004_5565;
const UI_SET_RELBIT: u64 = 0x4004_5566;
const UI_DEV_SETUP: u64 = 0x405c_5503;
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;
// <linux/input-event-codes.h>
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const SYN_REPORT: u16 = 0;
const REL_X: i32 = 0;
const REL_Y: i32 = 1;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BUS_USB: u16 = 0x03;

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}
#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

/// Whether this user may write to `/dev/uinput` (no device is created).
pub fn writable() -> bool {
    let Ok(path) = std::ffi::CString::new(PATH) else {
        return false;
    };
    // SAFETY: plain access(2) on a NUL-terminated path.
    Path::new(PATH).exists() && unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0
}

pub struct Mouse {
    file: File,
}

fn ioctl(fd: i32, request: u64, arg: libc::c_ulong) -> std::io::Result<()> {
    // SAFETY: the uinput ioctls used here take an int or a pointer to a live struct.
    if unsafe { libc::ioctl(fd, request as _, arg) } < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl Mouse {
    pub fn create() -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(PATH)?;
        let fd = file.as_raw_fd();
        ioctl(fd, UI_SET_EVBIT, EV_KEY.into())?;
        ioctl(fd, UI_SET_EVBIT, EV_REL.into())?;
        ioctl(fd, UI_SET_EVBIT, EV_SYN.into())?;
        for button in [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE] {
            ioctl(fd, UI_SET_KEYBIT, button.into())?;
        }
        ioctl(fd, UI_SET_RELBIT, REL_X as libc::c_ulong)?;
        ioctl(fd, UI_SET_RELBIT, REL_Y as libc::c_ulong)?;
        let mut setup = UinputSetup {
            id: InputId {
                bustype: BUS_USB,
                vendor: 0x1d6b,
                product: 0x0f0c,
                version: 1,
            },
            name: [0; 80],
            ff_effects_max: 0,
        };
        setup.name[..NAME.len()].copy_from_slice(NAME);
        ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup as libc::c_ulong)?;
        ioctl(fd, UI_DEV_CREATE, 0)?;
        Ok(Self { file })
    }

    fn emit(&self, kind: u16, code: u16, value: i32) -> std::io::Result<()> {
        // struct input_event: timeval, type, code, value. A zero time lets the kernel stamp it.
        const T: usize = std::mem::size_of::<libc::timeval>();
        let mut buf = [0u8; T + 8];
        buf[T..T + 2].copy_from_slice(&kind.to_ne_bytes());
        buf[T + 2..T + 4].copy_from_slice(&code.to_ne_bytes());
        buf[T + 4..T + 8].copy_from_slice(&value.to_ne_bytes());
        (&self.file).write_all(&buf)
    }

    pub fn button(&self, button: ClickButton, down: bool) -> std::io::Result<()> {
        let code = match button {
            ClickButton::Left => BTN_LEFT,
            ClickButton::Right => BTN_RIGHT,
            ClickButton::Middle => BTN_MIDDLE,
        };
        self.emit(EV_KEY, code, i32::from(down))?;
        self.emit(EV_SYN, SYN_REPORT, 0)
    }
}

impl Drop for Mouse {
    fn drop(&mut self) {
        let _ = ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY, 0);
    }
}

#[cfg(test)]
mod tests {
    /// Real device check (no clicks are sent): `cargo test --lib uinput -- --ignored`.
    #[test]
    #[ignore = "creates a real /dev/uinput device"]
    fn creates_a_virtual_mouse_the_kernel_lists() {
        assert!(super::writable(), "/dev/uinput is not writable for this user");
        let mouse = super::Mouse::create().expect("create");
        std::thread::sleep(std::time::Duration::from_millis(200));
        let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap();
        assert!(devices.contains("Monolith autoclicker"), "device not listed");
        drop(mouse);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap();
        assert!(!devices.contains("Monolith autoclicker"), "device not destroyed");
    }
}
