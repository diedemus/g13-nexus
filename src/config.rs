use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LcdAlign {
    Left,
    Center,
    Right,
}
impl Default for LcdAlign {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub key: String,
    pub down: bool,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Bank {
    pub bindings: BTreeMap<String, String>,
    pub macros: BTreeMap<String, Vec<MacroStep>>,
}

fn default_lcd_enabled() -> bool {
    true
}
fn default_lcd_page() -> u8 {
    0
}
fn default_lcd_align() -> [LcdAlign; 4] {
    [LcdAlign::Left; 4]
}
fn default_lcd_image_path() -> String {
    String::new()
}
fn default_lcd_image_scale() -> f32 {
    1.0
}
fn default_lcd_image_zoom() -> f32 {
    1.0
}
fn default_lcd_image_anchor() -> f32 {
    0.0
}
fn default_lcd_lines() -> [String; 4] {
    [
        "G13 NEXUS".into(),
        "Custom LCD page".into(),
        String::new(),
        String::new(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub color: [u8; 3],
    pub deadzone: u8,
    #[serde(default)]
    pub joystick_center_x: Option<u8>,
    #[serde(default)]
    pub joystick_center_y: Option<u8>,
    pub active_bank: u8,
    pub banks: [Bank; 3],
    #[serde(default = "default_lcd_enabled")]
    pub lcd_enabled: bool,
    #[serde(default = "default_lcd_page")]
    pub lcd_page: u8,
    #[serde(default = "default_lcd_lines")]
    pub lcd_lines: [String; 4],
    #[serde(default = "default_lcd_align")]
    pub lcd_align: [LcdAlign; 4],
    #[serde(default = "default_lcd_image_path")]
    pub lcd_image_path: String,
    #[serde(default = "default_lcd_image_scale")]
    pub lcd_image_scale_x: f32,
    #[serde(default = "default_lcd_image_scale")]
    pub lcd_image_scale_y: f32,
    #[serde(default = "default_lcd_image_zoom")]
    pub lcd_image_zoom: f32,
    #[serde(default = "default_lcd_image_anchor")]
    pub lcd_image_anchor_x: f32,
    #[serde(default = "default_lcd_image_anchor")]
    pub lcd_image_anchor_y: f32,
    #[serde(default)]
    pub lcd_text: String,
}

impl Default for Profile {
    fn default() -> Self {
        let mut bank = Bank::default();
        for i in 1..=22 {
            bank.bindings
                .insert(format!("G{i}"), format!("KEY_F{}", ((i - 1) % 12) + 1));
        }
        for (key, action) in [
            ("ABS_Y-", "KEY_W"),
            ("ABS_Y+", "KEY_S"),
            ("ABS_X-", "KEY_A"),
            ("ABS_X+", "KEY_D"),
            ("BTN_BASE", "KEY_LEFTCTRL"),
            ("BTN_BASE2", "KEY_SPACE"),
            ("BTN_THUMB", "KEY_ENTER"),
        ] {
            bank.bindings.insert(key.into(), action.into());
        }
        Self {
            name: "Default".into(),
            color: [0, 168, 255],
            deadzone: 38,
            joystick_center_x: None,
            joystick_center_y: None,
            active_bank: 1,
            banks: [bank.clone(), bank.clone(), bank],
            lcd_enabled: true,
            lcd_page: 0,
            lcd_lines: default_lcd_lines(),
            lcd_align: default_lcd_align(),
            lcd_image_path: default_lcd_image_path(),
            lcd_image_scale_x: 1.0,
            lcd_image_scale_y: 1.0,
            lcd_image_zoom: 1.0,
            lcd_image_anchor_x: 0.0,
            lcd_image_anchor_y: 0.0,
            lcd_text: String::new(),
        }
    }
}

impl Profile {
    pub fn bank(&self) -> &Bank {
        &self.banks[(self.active_bank.clamp(1, 3) - 1) as usize]
    }
    pub fn bank_mut(&mut self) -> &mut Bank {
        &mut self.banks[(self.active_bank.clamp(1, 3) - 1) as usize]
    }
    pub fn normalize(&mut self) {
        self.active_bank = self.active_bank.clamp(1, 3);
        self.lcd_page %= 4;
        self.lcd_image_scale_x = self.lcd_image_scale_x.clamp(0.25, 3.0);
        self.lcd_image_scale_y = self.lcd_image_scale_y.clamp(0.25, 3.0);
        self.lcd_image_zoom = self.lcd_image_zoom.clamp(0.25, 4.0);
        self.lcd_image_anchor_x = self.lcd_image_anchor_x.clamp(-1.0, 1.0);
        self.lcd_image_anchor_y = self.lcd_image_anchor_y.clamp(-1.0, 1.0);
        if !self.lcd_text.trim().is_empty() && self.lcd_lines[0] == "G13 NEXUS" {
            self.lcd_lines[0] = self.lcd_text.clone();
            self.lcd_text.clear();
        }
        for bank in &mut self.banks {
            migrate_binding_alias(bank, "THUMB_LEFT", "BTN_BASE");
            migrate_binding_alias(bank, "THUMB_RIGHT", "BTN_BASE2");
            migrate_binding_alias(bank, "STICK_CLICK", "BTN_THUMB");
            migrate_binding_alias(bank, "STICK_UP", "ABS_Y-");
            migrate_binding_alias(bank, "STICK_DOWN", "ABS_Y+");
            migrate_binding_alias(bank, "STICK_LEFT", "ABS_X-");
            migrate_binding_alias(bank, "STICK_RIGHT", "ABS_X+");
            bank.bindings.remove("THUMB_BOTTOM");
            bank.macros.remove("THUMB_BOTTOM");
        }
    }
}

fn migrate_binding_alias(bank: &mut Bank, old: &str, new: &str) {
    if !bank.bindings.contains_key(new) {
        if let Some(value) = bank.bindings.remove(old) {
            bank.bindings.insert(new.into(), value);
        }
    } else {
        bank.bindings.remove(old);
    }
    if !bank.macros.contains_key(new) {
        if let Some(value) = bank.macros.remove(old) {
            bank.macros.insert(new.into(), value);
        }
    } else {
        bank.macros.remove(old);
    }
}

pub fn dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("g13-nexus")
}
pub fn profiles_dir() -> PathBuf {
    dir().join("profiles")
}
fn active_path() -> PathBuf {
    dir().join("active-profile")
}
fn legacy_path() -> PathBuf {
    dir().join("profile.json")
}

fn safe_file_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            out.push(ch);
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        "Default".into()
    } else {
        out.chars().take(80).collect()
    }
}

fn profile_path(name: &str) -> PathBuf {
    profiles_dir().join(format!("{}.json", safe_file_name(name)))
}

fn parse_profile(path: &Path) -> Option<Profile> {
    let s = fs::read_to_string(path).ok()?;
    if let Ok(mut p) = serde_json::from_str::<Profile>(&s) {
        p.normalize();
        return Some(p);
    }
    #[derive(Deserialize)]
    struct Old {
        name: String,
        color: [u8; 3],
        deadzone: u8,
        bindings: BTreeMap<String, String>,
    }
    if let Ok(old) = serde_json::from_str::<Old>(&s) {
        let bank = Bank {
            bindings: old.bindings,
            macros: BTreeMap::new(),
        };
        let mut p = Profile {
            name: old.name,
            color: old.color,
            deadzone: old.deadzone,
            joystick_center_x: None,
            joystick_center_y: None,
            active_bank: 1,
            banks: [bank.clone(), bank.clone(), bank],
            lcd_enabled: true,
            lcd_page: 0,
            lcd_lines: default_lcd_lines(),
            lcd_align: default_lcd_align(),
            lcd_image_path: String::new(),
            lcd_image_scale_x: 1.0,
            lcd_image_scale_y: 1.0,
            lcd_image_zoom: 1.0,
            lcd_image_anchor_x: 0.0,
            lcd_image_anchor_y: 0.0,
            lcd_text: String::new(),
        };
        p.normalize();
        return Some(p);
    }
    None
}

fn ensure_store() -> io::Result<()> {
    fs::create_dir_all(profiles_dir())?;
    let has_profiles = fs::read_dir(profiles_dir())?
        .flatten()
        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"));
    if !has_profiles {
        let mut p = parse_profile(&legacy_path()).unwrap_or_default();
        if p.name.trim().is_empty() {
            p.name = "Default".into();
        }
        save_named(&p.name.clone(), &p)?;
        set_active(&p.name)?;
    } else if !active_path().exists() {
        let first = list_profiles()
            .into_iter()
            .next()
            .unwrap_or_else(|| "Default".into());
        set_active(&first)?;
    }
    Ok(())
}

pub fn list_profiles() -> Vec<String> {
    let _ = fs::create_dir_all(profiles_dir());
    let mut names: Vec<String> = fs::read_dir(profiles_dir())
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                return None;
            }
            path.file_stem()
                .and_then(|x| x.to_str())
                .map(|x| x.to_string())
        })
        .collect();
    names.sort_by_key(|s| s.to_ascii_lowercase());
    names
}

pub fn active_name() -> String {
    let _ = ensure_store();
    fs::read_to_string(active_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Default".into())
}

pub fn set_active(name: &str) -> io::Result<()> {
    fs::create_dir_all(dir())?;
    fs::write(active_path(), safe_file_name(name))
}

pub fn load_named(name: &str) -> Option<Profile> {
    let mut p = parse_profile(&profile_path(name))?;
    p.name = safe_file_name(name);
    Some(p)
}

pub fn load() -> Profile {
    let _ = ensure_store();
    let active = active_name();
    if let Some(p) = load_named(&active) {
        return p;
    }
    let names = list_profiles();
    if let Some(name) = names.first() {
        let _ = set_active(name);
        if let Some(p) = load_named(name) {
            return p;
        }
    }
    Profile::default()
}

pub fn save_named(name: &str, p: &Profile) -> io::Result<()> {
    fs::create_dir_all(profiles_dir())?;
    let name = safe_file_name(name);
    let mut copy = p.clone();
    copy.name = name.clone();
    copy.normalize();
    fs::write(
        profile_path(&name),
        serde_json::to_vec_pretty(&copy).unwrap(),
    )
}

pub fn save(p: &Profile) -> io::Result<()> {
    let name = active_name();
    save_named(&name, p)
}

pub fn save_as(name: &str, p: &Profile) -> io::Result<Profile> {
    let name = safe_file_name(name);
    save_named(&name, p)?;
    set_active(&name)?;
    load_named(&name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "saved profile missing"))
}

pub fn rename_active(new_name: &str, p: &Profile) -> io::Result<Profile> {
    let old = active_name();
    let new_name = safe_file_name(new_name);
    if new_name != old && profile_path(&new_name).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "profile already exists",
        ));
    }
    let result = save_as(&new_name, p)?;
    if new_name != old {
        let _ = fs::remove_file(profile_path(&old));
    }
    Ok(result)
}

pub fn delete_active() -> io::Result<Profile> {
    let current = active_name();
    let all = list_profiles();
    if all.len() <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "cannot delete the last profile",
        ));
    }
    fs::remove_file(profile_path(&current))?;
    let next = all.into_iter().find(|n| n != &current).unwrap();
    set_active(&next)?;
    load_named(&next).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "next profile missing"))
}
