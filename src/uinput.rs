use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::{AsRawFd, RawFd};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0x00;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;

#[repr(C)]
#[derive(Copy, Clone)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}
#[repr(C)]
struct UInputUserDev {
    name: [u8; 80],
    id: InputId,
    ff_effects_max: u32,
    absmax: [i32; 64],
    absmin: [i32; 64],
    absfuzz: [i32; 64],
    absflat: [i32; 64],
}

pub struct VirtualKeyboard {
    file: File,
    owner: bool,
}
impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/uinput")
            .context("open /dev/uinput")?;
        let fd = file.as_raw_fd();
        ioctl(fd, UI_SET_EVBIT, EV_KEY as _)?;
        ioctl(fd, UI_SET_EVBIT, EV_SYN as _)?;
        for key in 1u16..=767 {
            ioctl(fd, UI_SET_KEYBIT, key as _).with_context(|| format!("enable key code {key}"))?;
        }
        let mut device: UInputUserDev = unsafe { std::mem::zeroed() };
        let name = b"G13 Nexus Virtual Keyboard";
        device.name[..name.len()].copy_from_slice(name);
        device.id = InputId {
            bustype: 0x03,
            vendor: 0x1234,
            product: 0x5678,
            version: 0x0200,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&device as *const UInputUserDev).cast::<u8>(),
                std::mem::size_of::<UInputUserDev>(),
            )
        };
        file.write_all(bytes)?;
        if unsafe { libc::ioctl(fd, UI_DEV_CREATE) } < 0 {
            return Err(std::io::Error::last_os_error()).context("UI_DEV_CREATE");
        }
        Ok(Self { file, owner: true })
    }
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            file: self.file.try_clone().context("clone uinput fd")?,
            owner: false,
        })
    }
    pub fn key(&self, code: u16, value: i32) -> Result<()> {
        self.emit(EV_KEY, code, value)?;
        self.emit(EV_SYN, SYN_REPORT, 0)
    }
    fn emit(&self, event_type: u16, code: u16, value: i32) -> Result<()> {
        let event = libc::input_event {
            time: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            type_: event_type,
            code,
            value,
        };
        let size = std::mem::size_of::<libc::input_event>();
        let written = unsafe {
            libc::write(
                self.file.as_raw_fd(),
                (&event as *const libc::input_event).cast(),
                size,
            )
        };
        if written < 0 {
            return Err(std::io::Error::last_os_error()).context("write uinput event");
        }
        if written as usize != size {
            anyhow::bail!("short write to /dev/uinput")
        }
        Ok(())
    }
}
impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        if self.owner {
            unsafe {
                libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
            }
        }
    }
}
fn ioctl(fd: RawFd, request: libc::c_ulong, arg: libc::c_ulong) -> Result<()> {
    if unsafe { libc::ioctl(fd, request, arg) } < 0 {
        Err(std::io::Error::last_os_error()).context("uinput ioctl")
    } else {
        Ok(())
    }
}

pub fn keycode(name: &str) -> Option<u16> {
    let n = name.trim().strip_prefix("KEY_").unwrap_or(name.trim());
    match n {
        "ESC" => Some(1),
        "1" => Some(2),
        "2" => Some(3),
        "3" => Some(4),
        "4" => Some(5),
        "5" => Some(6),
        "6" => Some(7),
        "7" => Some(8),
        "8" => Some(9),
        "9" => Some(10),
        "0" => Some(11),
        "MINUS" => Some(12),
        "EQUAL" => Some(13),
        "BACKSPACE" => Some(14),
        "TAB" => Some(15),
        "Q" => Some(16),
        "W" => Some(17),
        "E" => Some(18),
        "R" => Some(19),
        "T" => Some(20),
        "Y" => Some(21),
        "U" => Some(22),
        "I" => Some(23),
        "O" => Some(24),
        "P" => Some(25),
        "LEFTBRACE" => Some(26),
        "RIGHTBRACE" => Some(27),
        "ENTER" => Some(28),
        "LEFTCTRL" => Some(29),
        "A" => Some(30),
        "S" => Some(31),
        "D" => Some(32),
        "F" => Some(33),
        "G" => Some(34),
        "H" => Some(35),
        "J" => Some(36),
        "K" => Some(37),
        "L" => Some(38),
        "SEMICOLON" => Some(39),
        "APOSTROPHE" => Some(40),
        "GRAVE" => Some(41),
        "LEFTSHIFT" => Some(42),
        "BACKSLASH" => Some(43),
        "Z" => Some(44),
        "X" => Some(45),
        "C" => Some(46),
        "V" => Some(47),
        "B" => Some(48),
        "N" => Some(49),
        "M" => Some(50),
        "COMMA" => Some(51),
        "DOT" => Some(52),
        "SLASH" => Some(53),
        "RIGHTSHIFT" => Some(54),
        "LEFTALT" => Some(56),
        "SPACE" => Some(57),
        "CAPSLOCK" => Some(58),
        "F1" => Some(59),
        "F2" => Some(60),
        "F3" => Some(61),
        "F4" => Some(62),
        "F5" => Some(63),
        "F6" => Some(64),
        "F7" => Some(65),
        "F8" => Some(66),
        "F9" => Some(67),
        "F10" => Some(68),
        "F11" => Some(87),
        "F12" => Some(88),
        "RIGHTCTRL" => Some(97),
        "RIGHTALT" => Some(100),
        "HOME" => Some(102),
        "UP" => Some(103),
        "PAGEUP" => Some(104),
        "LEFT" => Some(105),
        "RIGHT" => Some(106),
        "END" => Some(107),
        "DOWN" => Some(108),
        "PAGEDOWN" => Some(109),
        "INSERT" => Some(110),
        "DELETE" => Some(111),
        "LEFTMETA" => Some(125),
        "RIGHTMETA" => Some(126),
        _ => n.parse::<u16>().ok(),
    }
}

pub fn keyname(code: u16) -> Option<&'static str> {
    match code {
        1 => Some("KEY_ESC"),
        2 => Some("KEY_1"),
        3 => Some("KEY_2"),
        4 => Some("KEY_3"),
        5 => Some("KEY_4"),
        6 => Some("KEY_5"),
        7 => Some("KEY_6"),
        8 => Some("KEY_7"),
        9 => Some("KEY_8"),
        10 => Some("KEY_9"),
        11 => Some("KEY_0"),
        12 => Some("KEY_MINUS"),
        13 => Some("KEY_EQUAL"),
        14 => Some("KEY_BACKSPACE"),
        15 => Some("KEY_TAB"),
        16 => Some("KEY_Q"),
        17 => Some("KEY_W"),
        18 => Some("KEY_E"),
        19 => Some("KEY_R"),
        20 => Some("KEY_T"),
        21 => Some("KEY_Y"),
        22 => Some("KEY_U"),
        23 => Some("KEY_I"),
        24 => Some("KEY_O"),
        25 => Some("KEY_P"),
        26 => Some("KEY_LEFTBRACE"),
        27 => Some("KEY_RIGHTBRACE"),
        28 => Some("KEY_ENTER"),
        29 => Some("KEY_LEFTCTRL"),
        30 => Some("KEY_A"),
        31 => Some("KEY_S"),
        32 => Some("KEY_D"),
        33 => Some("KEY_F"),
        34 => Some("KEY_G"),
        35 => Some("KEY_H"),
        36 => Some("KEY_J"),
        37 => Some("KEY_K"),
        38 => Some("KEY_L"),
        39 => Some("KEY_SEMICOLON"),
        40 => Some("KEY_APOSTROPHE"),
        41 => Some("KEY_GRAVE"),
        42 => Some("KEY_LEFTSHIFT"),
        43 => Some("KEY_BACKSLASH"),
        44 => Some("KEY_Z"),
        45 => Some("KEY_X"),
        46 => Some("KEY_C"),
        47 => Some("KEY_V"),
        48 => Some("KEY_B"),
        49 => Some("KEY_N"),
        50 => Some("KEY_M"),
        51 => Some("KEY_COMMA"),
        52 => Some("KEY_DOT"),
        53 => Some("KEY_SLASH"),
        54 => Some("KEY_RIGHTSHIFT"),
        56 => Some("KEY_LEFTALT"),
        57 => Some("KEY_SPACE"),
        58 => Some("KEY_CAPSLOCK"),
        59 => Some("KEY_F1"),
        60 => Some("KEY_F2"),
        61 => Some("KEY_F3"),
        62 => Some("KEY_F4"),
        63 => Some("KEY_F5"),
        64 => Some("KEY_F6"),
        65 => Some("KEY_F7"),
        66 => Some("KEY_F8"),
        67 => Some("KEY_F9"),
        68 => Some("KEY_F10"),
        87 => Some("KEY_F11"),
        88 => Some("KEY_F12"),
        97 => Some("KEY_RIGHTCTRL"),
        100 => Some("KEY_RIGHTALT"),
        102 => Some("KEY_HOME"),
        103 => Some("KEY_UP"),
        104 => Some("KEY_PAGEUP"),
        105 => Some("KEY_LEFT"),
        106 => Some("KEY_RIGHT"),
        107 => Some("KEY_END"),
        108 => Some("KEY_DOWN"),
        109 => Some("KEY_PAGEDOWN"),
        110 => Some("KEY_INSERT"),
        111 => Some("KEY_DELETE"),
        125 => Some("KEY_LEFTMETA"),
        126 => Some("KEY_RIGHTMETA"),
        _ => None,
    }
}
