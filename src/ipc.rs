use crate::config::{LcdAlign, MacroStep};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Reload,
    SetColor { rgb: [u8; 3] },
    SetBinding { key: String, action: String },
    SetDeadzone { value: u8 },
    SetJoystickCenter { x: Option<u8>, y: Option<u8> },
    SetBank { bank: u8 },
    LoadProfile { name: String },
    SaveProfileAs { name: String },
    RenameProfile { name: String },
    DeleteProfile,
    SetLcdEnabled { enabled: bool },
    SetLcdPage { page: u8 },
    SetLcdLines { lines: [String; 4] },
    SetLcdAlign { align: [LcdAlign; 4] },
    SetLcdImage { path: String },
    SetLcdImageTransform { scale_x: f32, scale_y: f32, zoom: f32, anchor_x: f32, anchor_y: f32 },
    LcdRefresh,
    RecordToggle,
    RecordTarget { key: String },
    RecordEvent { key: String, down: bool },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
    pub connected: bool,
    pub x: u8,
    pub y: u8,
    #[serde(default = "default_axis_center")]
    pub center_x: u8,
    #[serde(default = "default_axis_center")]
    pub center_y: u8,
    pub pressed: Vec<String>,
    pub active_bank: u8,
    pub recording: bool,
    pub record_target: Option<String>,
    #[serde(default)]
    pub record_preview: Vec<MacroStep>,
    #[serde(default)]
    pub macro_targets: Vec<String>,
    pub lcd_ok: bool,
    pub lcd_enabled: bool,
    pub lcd_page: u8,
    pub profile_name: String,
    pub profiles: Vec<String>,
}

fn default_axis_center() -> u8 { 127 }

pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("g13-nexus.sock")
}
