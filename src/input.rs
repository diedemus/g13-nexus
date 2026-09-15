use anyhow::{Context, Result};
use std::{
    fs,
    fs::File,
    io::Read,
    os::fd::AsRawFd,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;

pub const KEY_LIGHTS_TOGGLE: u16 = 0x21e;

pub const BTN_THUMB: u16 = 0x121;
pub const BTN_BASE: u16 = 0x126;
pub const BTN_BASE2: u16 = 0x127;

pub const KEY_MACRO1: u16 = 0x290;
pub const KEY_MACRO22: u16 = 0x2a5;
pub const KEY_MACRO_RECORD_START: u16 = 0x2b0;
pub const KEY_MACRO_PRESET1: u16 = 0x2b3;
pub const KEY_MACRO_PRESET2: u16 = 0x2b4;
pub const KEY_MACRO_PRESET3: u16 = 0x2b5;
pub const KEY_KBD_LCD_MENU1: u16 = 0x2b8;
pub const KEY_KBD_LCD_MENU4: u16 = 0x2bb;
pub const KEY_KBD_LCD_MENU5: u16 = 0x2bc;

const EVIOCGRAB: libc::c_ulong = 0x4004_4590;
const EVIOCGABS_X: libc::c_ulong = 0x8018_4540;
const EVIOCGABS_Y: libc::c_ulong = 0x8018_4541;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct InputAbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Keypad,
    Thumbstick,
}

pub struct InputDevice {
    pub kind: DeviceKind,
    pub path: PathBuf,
    file: File,
    grabbed: bool,
}

impl InputDevice {
    pub fn open(kind: DeviceKind, path: PathBuf) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        Ok(Self {
            kind,
            path,
            file,
            grabbed: false,
        })
    }

    pub fn grab_exclusive(&mut self) -> Result<()> {
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCGRAB, 1) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("EVIOCGRAB {}", self.path.display()));
        }
        self.grabbed = true;
        Ok(())
    }

    pub fn abs_value(&self, axis: u16) -> Result<i32> {
        let request = match axis {
            ABS_X => EVIOCGABS_X,
            ABS_Y => EVIOCGABS_Y,
            _ => anyhow::bail!("unsupported ABS axis {axis}"),
        };
        let mut info = InputAbsInfo::default();
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), request, &mut info) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("EVIOCGABS({axis}) {}", self.path.display()));
        }
        Ok(info.value)
    }

    pub fn read_events(&mut self) -> Result<Vec<libc::input_event>> {
        let event_size = std::mem::size_of::<libc::input_event>();
        let mut buf = vec![0u8; event_size * 64];
        match self.file.read(&mut buf) {
            Ok(0) => anyhow::bail!("{} disconnected", self.path.display()),
            Ok(n) => {
                let mut out = Vec::new();
                for chunk in buf[..n].chunks_exact(event_size) {
                    let ev = unsafe {
                        std::ptr::read_unaligned(chunk.as_ptr().cast::<libc::input_event>())
                    };
                    out.push(ev);
                }
                Ok(out)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("read {}", self.path.display())),
        }
    }
}

impl Drop for InputDevice {
    fn drop(&mut self) {
        if self.grabbed {
            unsafe {
                libc::ioctl(self.file.as_raw_fd(), EVIOCGRAB, 0);
            }
        }
    }
}

pub struct InputTap {
    pub path: PathBuf,
    file: File,
}

impl InputTap {
    pub fn open(path: PathBuf) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        Ok(Self { path, file })
    }

    pub fn read_events(&mut self) -> Result<Vec<libc::input_event>> {
        let event_size = std::mem::size_of::<libc::input_event>();
        let mut buf = vec![0u8; event_size * 64];
        match self.file.read(&mut buf) {
            Ok(0) => anyhow::bail!("{} disconnected", self.path.display()),
            Ok(n) => {
                let mut out = Vec::new();
                for chunk in buf[..n].chunks_exact(event_size) {
                    let ev = unsafe {
                        std::ptr::read_unaligned(chunk.as_ptr().cast::<libc::input_event>())
                    };
                    out.push(ev);
                }
                Ok(out)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("read {}", self.path.display())),
        }
    }
}

pub fn keyboard_event_nodes() -> Result<Vec<PathBuf>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir("/sys/class/input").context("read /sys/class/input")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") || belongs_to_g13(&entry.path()) {
            continue;
        }
        let dev_name = fs::read_to_string(entry.path().join("device/name")).unwrap_or_default();
        if dev_name.trim() == "G13 Nexus Virtual Keyboard" {
            continue;
        }
        // Only consider devices exposing EV_KEY capabilities. Individual events
        // are filtered again by the keyboard keycode map in the daemon.
        let key_caps = entry.path().join("device/capabilities/key");
        if !key_caps.exists() {
            continue;
        }
        nodes.push(PathBuf::from("/dev/input").join(name.as_ref()));
    }
    nodes.sort();
    nodes.dedup();
    Ok(nodes)
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
}

fn belongs_to_g13(event_entry: &Path) -> bool {
    let Ok(mut current) = fs::canonicalize(event_entry.join("device")) else {
        return false;
    };

    loop {
        let vendor = read_trimmed(&current.join("idVendor"));
        let product = read_trimmed(&current.join("idProduct"));
        if vendor.as_deref() == Some("046d") && product.as_deref() == Some("c21c") {
            return true;
        }
        if !current.pop() {
            break;
        }
    }
    false
}

pub fn g13_event_nodes() -> Result<Vec<PathBuf>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir("/sys/class/input").context("read /sys/class/input")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        if belongs_to_g13(&entry.path()) {
            nodes.push(PathBuf::from("/dev/input").join(name.as_ref()));
        }
    }
    nodes.sort();
    nodes.dedup();
    Ok(nodes)
}

pub fn discover() -> Result<(PathBuf, PathBuf)> {
    let mut keypad = None;
    let mut stick = None;
    for entry in fs::read_dir("/sys/class/input").context("read /sys/class/input")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        if !belongs_to_g13(&entry.path()) {
            continue;
        }

        let dev_name = fs::read_to_string(entry.path().join("device/name")).unwrap_or_default();
        let dev = PathBuf::from("/dev/input").join(name.as_ref());
        match dev_name.trim() {
            "Logitech G13 Gaming Keypad" => keypad = Some(dev),
            "Logitech G13 Thumbstick" => stick = Some(dev),
            _ => {}
        }
    }
    match (keypad, stick) {
        (Some(k), Some(s)) => Ok((k, s)),
        _ => anyhow::bail!("kernel G13 input devices not found for USB 046d:c21c"),
    }
}

pub fn key_control(code: u16) -> Option<String> {
    if (KEY_MACRO1..=KEY_MACRO22).contains(&code) {
        return Some(format!("G{}", code - KEY_MACRO1 + 1));
    }
    match code {
        KEY_LIGHTS_TOGGLE => Some("KEY_LIGHTS_TOGGLE".into()),
        KEY_MACRO_PRESET1 => Some("KEY_MACRO_PRESET1".into()),
        KEY_MACRO_PRESET2 => Some("KEY_MACRO_PRESET2".into()),
        KEY_MACRO_PRESET3 => Some("KEY_MACRO_PRESET3".into()),
        KEY_MACRO_RECORD_START => Some("KEY_MACRO_RECORD_START".into()),
        KEY_KBD_LCD_MENU1..=KEY_KBD_LCD_MENU5 => {
            Some(format!("KEY_KBD_LCD_MENU{}", code - KEY_KBD_LCD_MENU1 + 1))
        }
        BTN_BASE => Some("BTN_BASE".into()),
        BTN_BASE2 => Some("BTN_BASE2".into()),
        BTN_THUMB => Some("BTN_THUMB".into()),
        _ => None,
    }
}
