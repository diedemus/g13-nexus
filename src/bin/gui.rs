use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};
use g13_nexus::{
    config::{self, LcdAlign},
    ipc::{self, Reply, Request},
    lcd,
};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn request(request: Request) -> Option<Reply> {
    let mut stream = UnixStream::connect(ipc::socket_path()).ok()?;
    // A GUI refresh must never sit for seconds behind a slow daemon operation.
    // The daemon services clients concurrently; these bounds also keep egui's
    // main thread responsive if a socket operation misbehaves.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(120)));
    writeln!(stream, "{}", serde_json::to_string(&request).ok()?).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

fn configurable(name: &str) -> bool {
    !(name.starts_with("KEY_MACRO_PRESET") || name == "KEY_MACRO_RECORD_START" || name == "KEY_LIGHTS_TOGGLE")
}


fn macro_key_label(key: &str) -> String {
    match key {
        "KEY_LEFTCTRL" => "Left Ctrl".into(),
        "KEY_RIGHTCTRL" => "Right Ctrl".into(),
        "KEY_LEFTSHIFT" => "Left Shift".into(),
        "KEY_RIGHTSHIFT" => "Right Shift".into(),
        "KEY_LEFTALT" => "Left Alt".into(),
        "KEY_RIGHTALT" => "Right Alt".into(),
        "KEY_LEFTMETA" => "Left Meta".into(),
        "KEY_RIGHTMETA" => "Right Meta".into(),
        "KEY_SPACE" => "Space".into(),
        "KEY_ENTER" => "Enter".into(),
        "KEY_TAB" => "Tab".into(),
        "KEY_ESC" => "Escape".into(),
        "KEY_BACKSPACE" => "Backspace".into(),
        "KEY_DELETE" => "Delete".into(),
        "KEY_INSERT" => "Insert".into(),
        "KEY_HOME" => "Home".into(),
        "KEY_END" => "End".into(),
        "KEY_PAGEUP" => "Page Up".into(),
        "KEY_PAGEDOWN" => "Page Down".into(),
        "KEY_UP" => "Up".into(),
        "KEY_DOWN" => "Down".into(),
        "KEY_LEFT" => "Left".into(),
        "KEY_RIGHT" => "Right".into(),
        "KEY_MINUS" => "-".into(),
        "KEY_EQUAL" => "=".into(),
        "KEY_COMMA" => ",".into(),
        "KEY_DOT" => ".".into(),
        "KEY_SEMICOLON" => ";".into(),
        "KEY_APOSTROPHE" => "'".into(),
        "KEY_BACKSLASH" => "\\".into(),
        "KEY_SLASH" => "/".into(),
        "KEY_LEFTBRACE" => "[".into(),
        "KEY_RIGHTBRACE" => "]".into(),
        "KEY_GRAVE" => "`".into(),
        _ => key.strip_prefix("KEY_").unwrap_or(key).replace('_', " "),
    }
}

struct App {
    profile: config::Profile,
    selected: String,
    edit: String,
    status: Option<Reply>,
    last_poll: Instant,
    lcd_preview_key: String,
    lcd_preview_texture: Option<egui::TextureHandle>,
    lcd_preview_error: Option<String>,
    profile_name_edit: String,
    was_recording: bool,
    recorder_mods: [bool; 4],
}

impl Default for App {
    fn default() -> Self {
        let profile = config::load();
        Self {
            profile_name_edit: profile.name.clone(),
            profile,
            selected: String::new(),
            edit: String::new(),
            status: None,
            last_poll: Instant::now() - Duration::from_secs(2),
            lcd_preview_key: String::new(),
            lcd_preview_texture: None,
            lcd_preview_error: None,
            was_recording: false,
            recorder_mods: [false; 4],
        }
    }
}

fn egui_key_to_linux(key: egui::Key) -> Option<String> {
    let name = format!("{key:?}");
    if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
        return Some(format!("KEY_{}", name.to_ascii_uppercase()));
    }
    if let Some(digit) = name.strip_prefix("Num") {
        if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
            return Some(format!("KEY_{digit}"));
        }
    }
    if name.starts_with('F') && name[1..].parse::<u8>().ok().is_some_and(|n| (1..=12).contains(&n)) {
        return Some(format!("KEY_{name}"));
    }
    let mapped = match name.as_str() {
        "Escape" => "KEY_ESC",
        "Tab" => "KEY_TAB",
        "Backspace" => "KEY_BACKSPACE",
        "Enter" => "KEY_ENTER",
        "Space" => "KEY_SPACE",
        "Insert" => "KEY_INSERT",
        "Delete" => "KEY_DELETE",
        "Home" => "KEY_HOME",
        "End" => "KEY_END",
        "PageUp" => "KEY_PAGEUP",
        "PageDown" => "KEY_PAGEDOWN",
        "ArrowUp" => "KEY_UP",
        "ArrowDown" => "KEY_DOWN",
        "ArrowLeft" => "KEY_LEFT",
        "ArrowRight" => "KEY_RIGHT",
        "Minus" => "KEY_MINUS",
        "Equals" | "PlusEquals" => "KEY_EQUAL",
        "Comma" => "KEY_COMMA",
        "Period" | "Dot" => "KEY_DOT",
        "Semicolon" => "KEY_SEMICOLON",
        "Quote" | "Apostrophe" => "KEY_APOSTROPHE",
        "Backslash" => "KEY_BACKSLASH",
        "Slash" => "KEY_SLASH",
        "OpenBracket" => "KEY_LEFTBRACE",
        "CloseBracket" => "KEY_RIGHTBRACE",
        "Backtick" | "Grave" => "KEY_GRAVE",
        _ => return None,
    };
    Some(mapped.to_owned())
}

impl App {
    fn select(&mut self, key: &str) {
        self.selected = key.into();
        self.edit = self
            .profile
            .bank()
            .bindings
            .get(key)
            .cloned()
            .unwrap_or_default();
    }

    fn forward_recorder_keyboard(&mut self, ctx: &egui::Context) {
        let active = self.status.as_ref()
            .map(|status| status.recording && status.record_target.is_some())
            .unwrap_or(false);
        if !active {
            self.recorder_mods = [false; 4];
            return;
        }

        // Wayland intentionally prevents ordinary user applications from
        // globally keylogging the desktop. When the Nexus GUI has focus, send
        // its keyboard events to the daemon as the secure recorder source. The
        // daemon's raw-evdev recorder remains available on systems that grant
        // explicit keyboard ACLs, and duplicate edges are de-duplicated there.
        let modifiers = ctx.input(|input| input.modifiers);
        let current_mods = [modifiers.ctrl, modifiers.shift, modifiers.alt, modifiers.mac_cmd || modifiers.command];
        for (index, key) in ["KEY_LEFTCTRL", "KEY_LEFTSHIFT", "KEY_LEFTALT", "KEY_LEFTMETA"].iter().enumerate() {
            if current_mods[index] != self.recorder_mods[index] {
                let _ = request(Request::RecordEvent {
                    key: (*key).to_owned(),
                    down: current_mods[index],
                });
            }
        }
        self.recorder_mods = current_mods;

        let events = ctx.input(|input| input.events.clone());
        for event in events {
            if let egui::Event::Key { key, pressed, repeat, .. } = event {
                if repeat { continue; }
                if let Some(key) = egui_key_to_linux(key) {
                    let _ = request(Request::RecordEvent { key, down: pressed });
                }
            }
        }
    }

    fn update_lcd_image_preview(&mut self, ctx: &egui::Context) {
        if self.profile.lcd_page != 3 || self.profile.lcd_image_path.trim().is_empty() {
            self.lcd_preview_key.clear();
            self.lcd_preview_texture = None;
            self.lcd_preview_error = None;
            return;
        }

        let key = format!(
            "{}|{:.4}|{:.4}|{:.4}|{:.4}|{:.4}|{}|{}|{}",
            self.profile.lcd_image_path,
            self.profile.lcd_image_scale_x,
            self.profile.lcd_image_scale_y,
            self.profile.lcd_image_zoom,
            self.profile.lcd_image_anchor_x,
            self.profile.lcd_image_anchor_y,
            self.profile.color[0],
            self.profile.color[1],
            self.profile.color[2],
        );
        if key == self.lcd_preview_key {
            return;
        }
        self.lcd_preview_key = key;

        match lcd::image_preview(
            &self.profile.lcd_image_path,
            self.profile.lcd_image_scale_x,
            self.profile.lcd_image_scale_y,
            self.profile.lcd_image_zoom,
            self.profile.lcd_image_anchor_x,
            self.profile.lcd_image_anchor_y,
        ) {
            Ok(bits) => {
                let lcd_bg = Color32::from_rgb(
                    self.profile.color[0],
                    self.profile.color[1],
                    self.profile.color[2],
                );
                let lcd_ink = Color32::from_rgb(
                    (self.profile.color[0] as f32 * 0.10).round() as u8,
                    (self.profile.color[1] as f32 * 0.10).round() as u8,
                    (self.profile.color[2] as f32 * 0.10).round() as u8,
                );
                let pixels = bits
                    .into_iter()
                    .map(|on| if on { lcd_ink } else { lcd_bg })
                    .collect();
                let image = egui::ColorImage {
                    size: [lcd::W, lcd::H],
                    pixels,
                };
                self.lcd_preview_texture = Some(ctx.load_texture(
                    "g13-lcd-image-preview",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
                self.lcd_preview_error = None;
            }
            Err(err) => {
                self.lcd_preview_texture = None;
                self.lcd_preview_error = Some(err.to_string());
            }
        }
    }

    fn section_frame(&self, ui: &egui::Ui, rect: Rect, title: &str) {
        ui.painter().rect(
            rect,
            18.0,
            Color32::from_gray(20),
            Stroke::new(1.5_f32, Color32::from_gray(62)),
        );

        let title_w = (title.chars().count() as f32 * 7.6 + 28.0).min(rect.width() - 28.0);
        let title_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 14.0, rect.top() - 12.0),
            Vec2::new(title_w, 24.0),
        );
        ui.painter().rect(
            title_rect,
            10.0,
            Color32::from_gray(24),
            Stroke::new(1.0_f32, Color32::from_gray(52)),
        );
        ui.painter().text(
            title_rect.left_center() + Vec2::new(10.0, 0.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.5),
            Color32::from_gray(205),
        );
    }

    fn is_record_target(&self, name: &str) -> bool {
        self.status.as_ref()
            .map(|status| status.recording && status.record_target.as_deref() == Some(name))
            .unwrap_or(false)
    }

    fn macro_bound(&self, name: &str) -> bool {
        self.status.as_ref()
            .map(|status| status.macro_targets.iter().any(|target| target == name))
            .unwrap_or_else(|| self.profile.bank().macros.get(name).map(|steps| !steps.is_empty()).unwrap_or(false))
    }

    fn draw_macro_badge(&self, ui: &egui::Ui, rect: Rect, name: &str) {
        if !self.macro_bound(name) {
            return;
        }
        let badge = Rect::from_center_size(
            Pos2::new(rect.right() - 7.0, rect.top() + 7.0),
            Vec2::new(14.0, 14.0),
        );
        ui.painter().rect(
            badge,
            7.0,
            Color32::from_rgb(185, 115, 35),
            Stroke::new(1.0_f32, Color32::from_rgb(235, 180, 90)),
        );
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            "M",
            FontId::proportional(9.0),
            Color32::WHITE,
        );
    }

    fn control_button(&mut self, ui: &mut egui::Ui, rect: Rect, name: &str, label: &str) {
        let live = self
            .status
            .as_ref()
            .map(|s| s.pressed.iter().any(|pressed| pressed == name))
            .unwrap_or(false);
        let recording = name == "KEY_MACRO_RECORD_START"
            && self.status.as_ref().map(|s| s.recording).unwrap_or(false);
        let active_bank = self.status.as_ref().map(|s| s.active_bank).unwrap_or(self.profile.active_bank);
        let bank_selected = matches!(
            (name, active_bank),
            ("KEY_MACRO_PRESET1", 1) | ("KEY_MACRO_PRESET2", 2) | ("KEY_MACRO_PRESET3", 3)
        );
        let fill = if recording {
            Color32::from_rgb(165, 45, 45)
        } else if live {
            Color32::from_rgb(0, 190, 220)
        } else if bank_selected {
            Color32::from_rgb(70, 105, 145)
        } else if self.selected == name {
            Color32::from_rgb(70, 105, 145)
        } else {
            Color32::from_gray(45)
        };
        let target = self.is_record_target(name);
        let stroke = if target {
            Stroke::new(3.0_f32, Color32::from_rgb(255, 120, 55))
        } else {
            Stroke::new(1.0_f32, Color32::from_gray(110))
        };
        let response = ui.put(
            rect,
            egui::Button::new(RichText::new(label).size(11.0).family(egui::FontFamily::Proportional))
                .min_size(rect.size())
                .fill(fill)
                .stroke(stroke),
        );
        self.draw_macro_badge(ui, rect, name);
        if response.clicked() {
            if name == "KEY_MACRO_RECORD_START" {
                self.status = request(Request::RecordToggle);
                self.last_poll = Instant::now() - Duration::from_millis(34);
            } else if let Some(bank) = match name {
                "KEY_MACRO_PRESET1" => Some(1),
                "KEY_MACRO_PRESET2" => Some(2),
                "KEY_MACRO_PRESET3" => Some(3),
                _ => None,
            } {
                self.profile.active_bank = bank;
                self.status = request(Request::SetBank { bank });
                self.last_poll = Instant::now() - Duration::from_millis(34);
            } else if configurable(name) {
                self.select(name);
            }
        }
    }

    fn round_control_button(&mut self, ui: &mut egui::Ui, center: Pos2, radius: f32, name: &str, label: &str) {
        let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
        let response = ui.interact(rect, ui.id().with(name), Sense::click());
        let live = self.status.as_ref()
            .map(|s| s.pressed.iter().any(|pressed| pressed == name))
            .unwrap_or(false);
        let fill = if live {
            Color32::from_rgb(0, 190, 220)
        } else if self.selected == name {
            Color32::from_rgb(70, 105, 145)
        } else {
            Color32::from_gray(45)
        };
        let ring = if self.is_record_target(name) {
            Stroke::new(3.0_f32, Color32::from_rgb(255, 120, 55))
        } else {
            Stroke::new(1.2_f32, Color32::from_gray(110))
        };
        ui.painter().circle(center, radius, fill, ring);
        ui.painter().text(center, Align2::CENTER_CENTER, label, FontId::proportional(8.5), Color32::WHITE);
        self.draw_macro_badge(ui, rect, name);
        if response.clicked() { self.select(name); }
    }

    fn overlay_zone(&mut self, ui: &mut egui::Ui, rect: Rect, name: &str, label: &str) {
        let response = ui.interact(rect, ui.id().with(name), Sense::click());
        let live = self
            .status
            .as_ref()
            .map(|s| s.pressed.iter().any(|p| p == name))
            .unwrap_or(false);
        let fill = if live {
            Color32::from_rgb(0, 190, 220)
        } else if self.selected == name {
            Color32::from_rgb(70, 105, 145)
        } else {
            Color32::from_gray(42)
        };
        let border = if self.is_record_target(name) {
            Stroke::new(3.0_f32, Color32::from_rgb(255, 120, 55))
        } else {
            Stroke::new(1.5_f32, Color32::from_gray(105))
        };
        ui.painter().rect(
            rect,
            9.0,
            fill,
            border,
        );
        let c = rect.center();
        let tri = 9.0_f32;
        let triangle = match label {
            "▲" => Some(vec![
                Pos2::new(c.x, c.y - tri),
                Pos2::new(c.x - tri, c.y + tri * 0.75),
                Pos2::new(c.x + tri, c.y + tri * 0.75),
            ]),
            "▼" => Some(vec![
                Pos2::new(c.x, c.y + tri),
                Pos2::new(c.x - tri, c.y - tri * 0.75),
                Pos2::new(c.x + tri, c.y - tri * 0.75),
            ]),
            "◀" => Some(vec![
                Pos2::new(c.x - tri, c.y),
                Pos2::new(c.x + tri * 0.75, c.y - tri),
                Pos2::new(c.x + tri * 0.75, c.y + tri),
            ]),
            "▶" => Some(vec![
                Pos2::new(c.x + tri, c.y),
                Pos2::new(c.x - tri * 0.75, c.y - tri),
                Pos2::new(c.x - tri * 0.75, c.y + tri),
            ]),
            _ => None,
        };
        if let Some(points) = triangle {
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                Color32::WHITE,
                Stroke::NONE,
            ));
        } else {
            ui.painter().text(
                c,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(10.5),
                Color32::WHITE,
            );
        }
        self.draw_macro_badge(ui, rect, name);
        if response.clicked() {
            self.select(name);
        }
    }


}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Dropping a PNG/JPEG/BMP anywhere on the window loads it as the LCD image page.
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                let supported = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .map(|v| matches!(v.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp"))
                    .unwrap_or(false);
                if supported {
                    self.profile.lcd_image_path = path.to_string_lossy().into_owned();
                    self.profile.lcd_page = 3;
                    let _ = config::save(&self.profile);
                    let _ = request(Request::SetLcdImage {
                        path: self.profile.lcd_image_path.clone(),
                    });
                    let _ = request(Request::SetLcdPage { page: 3 });
                }
            }
        }

        if self.last_poll.elapsed() > Duration::from_millis(20) {
            self.status = request(Request::Status);
            self.last_poll = Instant::now();

            if let Some(status) = self.status.clone() {
                // The daemon is authoritative for the active hardware M bank. Update
                // it immediately so physical M1/M2/M3 presses are reflected in the
                // GUI without waiting for a profile reload from disk.
                let bank_changed = self.profile.active_bank != status.active_bank;
                if bank_changed {
                    self.profile.active_bank = status.active_bank;
                }

                if self.profile.lcd_page != status.lcd_page
                    || self.profile.lcd_enabled != status.lcd_enabled
                    || self.profile.name != status.profile_name
                {
                    self.profile = config::load();
                    self.profile_name_edit = self.profile.name.clone();
                    self.selected.clear();
                    self.edit.clear();
                }
                if let Some(key) = status.pressed.iter().find(|key| configurable(key)) {
                    if self.selected != *key {
                        self.select(key);
                    }
                }
                if self.was_recording && !status.recording {
                    // Do not reload from disk here: physical MR stop persists
                    // asynchronously, so an immediate read can race the save and
                    // restore stale bank/macro state. Daemon Status is authoritative.
                    self.profile.active_bank = status.active_bank;
                }
                self.was_recording = status.recording;
            }
            ctx.request_repaint_after(Duration::from_millis(20));
        }

        self.forward_recorder_keyboard(ctx);

        // Recording monitor is a floating sub-window so the approved G13
        // hardware layout remains untouched. It mirrors the daemon's accepted
        // MacroStep stream, regardless of whether events came from evdev or
        // the focused-GUI Wayland fallback.
        if let Some(status) = self.status.clone().filter(|status| status.recording) {
            egui::Window::new("Macro Recording")
                .id(egui::Id::new("macro_recording_window"))
                .default_size(Vec2::new(430.0, 300.0))
                .min_width(360.0)
                .min_height(220.0)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            Color32::from_rgb(255, 90, 90),
                            RichText::new("● RECORDING").strong().size(16.0),
                        );
                        ui.separator();
                        ui.label(RichText::new(format!("M{}", status.active_bank)).strong().size(16.0));
                    });
                    ui.add_space(8.0);

                    match &status.record_target {
                        Some(target) => {
                            egui::Frame::none()
                                .fill(Color32::from_rgb(70, 35, 20))
                                .stroke(Stroke::new(2.0_f32, Color32::from_rgb(255, 120, 55)))
                                .rounding(8.0)
                                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(RichText::new("RECORDING TO").strong().size(12.0));
                                        ui.label(RichText::new(target).strong().size(24.0).color(Color32::WHITE));
                                        ui.label(RichText::new(format!("M{} BANK", status.active_bank)).size(11.0));
                                    });
                                });
                        }
                        None => {
                            egui::Frame::none()
                                .fill(Color32::from_rgb(45, 45, 22))
                                .stroke(Stroke::new(2.0_f32, Color32::from_rgb(220, 180, 70)))
                                .rounding(8.0)
                                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(RichText::new("WAITING FOR DESTINATION").strong().size(16.0));
                                        ui.label("Press a programmable G13 control.");
                                    });
                                });
                        }
                    }

                    ui.add_space(8.0);
                    if status.record_target.is_none() {
                        ui.label("The destination control will be outlined in orange as soon as it is selected.");
                    } else {
                        ui.label(format!("{} recorded events", status.record_preview.len()));
                        ui.small("Press MR again to stop and save the macro.");
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if status.record_preview.is_empty() {
                                    ui.weak("Waiting for keyboard input…");
                                } else {
                                    for (index, step) in status.record_preview.iter().enumerate() {
                                        let edge = if step.down { "DOWN" } else { "UP" };
                                        ui.horizontal(|ui| {
                                            ui.monospace(format!("{:03}", index + 1));
                                            ui.label(RichText::new(edge).strong());
                                            ui.label(macro_key_label(&step.key));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.weak(format!("+{} ms", step.delay_ms));
                                            });
                                        });
                                    }
                                }
                            });
                    }
                });
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("G13 Nexus");
                ui.separator();
                if self.status.as_ref().map(|s| s.connected).unwrap_or(false) {
                    ui.label(
                        RichText::new("● G13 connected via kernel")
                            .color(Color32::from_rgb(80, 210, 120)),
                    );
                } else {
                    ui.label(
                        RichText::new("○ G13 unavailable")
                            .color(Color32::from_rgb(220, 100, 100)),
                    );
                }
                ui.separator();
                ui.label(format!("{} / M{}", self.profile.name, self.profile.active_bank));
                if self.status.as_ref().map(|s| s.recording).unwrap_or(false) {
                    ui.separator();
                    ui.label(
                        RichText::new("● MACRO RECORD")
                            .color(Color32::from_rgb(255, 90, 90)),
                    );
                }
            });
        });

        egui::SidePanel::right("editor")
            .exact_width(345.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Configuration");

                ui.label(RichText::new("Profile").strong());
                let profile_names = self.status.as_ref()
                    .map(|s| s.profiles.clone())
                    .unwrap_or_else(config::list_profiles);
                egui::ComboBox::from_id_source("profile_selector")
                    .selected_text(&self.profile.name)
                    .width(210.0)
                    .show_ui(ui, |ui| {
                        for name in &profile_names {
                            if ui.selectable_label(self.profile.name == *name, name).clicked() {
                                let _ = request(Request::LoadProfile { name: name.clone() });
                                self.profile = config::load();
                                self.profile_name_edit = self.profile.name.clone();
                                self.selected.clear();
                                self.edit.clear();
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.add_sized([170.0, 24.0], egui::TextEdit::singleline(&mut self.profile_name_edit));
                    if ui.button("Save As").clicked() && !self.profile_name_edit.trim().is_empty() {
                        let _ = request(Request::SaveProfileAs { name: self.profile_name_edit.clone() });
                        self.profile = config::load();
                        self.profile_name_edit = self.profile.name.clone();
                    }
                    if ui.button("Rename").clicked() && !self.profile_name_edit.trim().is_empty() {
                        let _ = request(Request::RenameProfile { name: self.profile_name_edit.clone() });
                        self.profile = config::load();
                        self.profile_name_edit = self.profile.name.clone();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(profile_names.len() > 1, egui::Button::new("Delete profile")).clicked() {
                        let _ = request(Request::DeleteProfile);
                        self.profile = config::load();
                        self.profile_name_edit = self.profile.name.clone();
                        self.selected.clear();
                        self.edit.clear();
                    }
                    ui.small(format!("{} profile{}", profile_names.len(), if profile_names.len() == 1 { "" } else { "s" }));
                });

                ui.separator();
                ui.label(RichText::new("M Bank").strong());
                ui.horizontal(|ui| {
                    for bank in 1..=3 {
                        if ui
                            .selectable_label(self.profile.active_bank == bank, format!("M{bank}"))
                            .clicked()
                        {
                            self.profile.active_bank = bank;
                            let _ = request(Request::SetBank { bank });
                        }
                    }
                });

                ui.separator();
                ui.label("Backlight color");
                let mut color = Color32::from_rgb(
                    self.profile.color[0],
                    self.profile.color[1],
                    self.profile.color[2],
                );
                if egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                )
                .changed()
                {
                    self.profile.color = [color.r(), color.g(), color.b()];
                    let _ = request(Request::SetColor {
                        rgb: self.profile.color,
                    });
                }

                ui.separator();
                ui.heading("LCD Display");
                let mut enabled = self.profile.lcd_enabled;
                if ui.checkbox(&mut enabled, "LCD enabled").changed() {
                    self.profile.lcd_enabled = enabled;
                    let _ = request(Request::SetLcdEnabled { enabled });
                }

                ui.horizontal(|ui| {
                    for (page, label) in [(0, "Status"), (1, "Custom"), (2, "Input Monitor"), (3, "Image")] {
                        if ui
                            .selectable_label(self.profile.lcd_page == page, label)
                            .clicked()
                        {
                            self.profile.lcd_page = page;
                            let _ = request(Request::SetLcdPage { page });
                        }
                    }
                });
                ui.small("LCD1: previous page   LCD2: next page   LCD3: on/off   LCD4: status/home");

                if self.profile.lcd_page == 1 {
                    ui.add_space(6.0);
                    ui.small("Each LCD line can be independently left, center, or right aligned.");
                    for i in 0..4 {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}", i + 1));
                            ui.add_sized([190.0, 24.0], egui::TextEdit::singleline(&mut self.profile.lcd_lines[i]));
                            for (align, label) in [
                                (LcdAlign::Left, "L"),
                                (LcdAlign::Center, "C"),
                                (LcdAlign::Right, "R"),
                            ] {
                                if ui.selectable_label(self.profile.lcd_align[i] == align, label).clicked() {
                                    self.profile.lcd_align[i] = align;
                                    let _ = config::save(&self.profile);
                                    let _ = request(Request::SetLcdAlign {
                                        align: self.profile.lcd_align,
                                    });
                                }
                            }
                        });
                    }
                    if ui.button("Apply LCD text").clicked() {
                        let _ = config::save(&self.profile);
                        let _ = request(Request::SetLcdLines {
                            lines: self.profile.lcd_lines.clone(),
                        });
                        let _ = request(Request::SetLcdAlign {
                            align: self.profile.lcd_align,
                        });
                    }
                } else if self.profile.lcd_page == 3 {
                    ui.add_space(6.0);
                    ui.small("PNG, JPEG, or BMP. Images are scaled proportionally to 160×43 and converted to monochrome.");
                    ui.small("Choose a file with the system file picker, or drag an image anywhere onto this window.");

                    let selected_name = std::path::Path::new(&self.profile.lcd_image_path)
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("No image selected");
                    ui.label(format!("Selected: {selected_name}"));

                    ui.horizontal(|ui| {
                        if ui.button("Browse…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Choose G13 LCD image")
                                .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                                .pick_file()
                            {
                                self.profile.lcd_image_path = path.to_string_lossy().into_owned();
                                self.profile.lcd_page = 3;
                                let _ = config::save(&self.profile);
                                let _ = request(Request::SetLcdImage {
                                    path: self.profile.lcd_image_path.clone(),
                                });
                                let _ = request(Request::SetLcdPage { page: 3 });
                                let _ = request(Request::LcdRefresh);
                            }
                        }
                        if !self.profile.lcd_image_path.is_empty()
                            && ui.button("Clear image").clicked()
                        {
                            self.profile.lcd_image_path.clear();
                            let _ = config::save(&self.profile);
                            let _ = request(Request::SetLcdImage { path: String::new() });
                            let _ = request(Request::LcdRefresh);
                        }
                    });
                    ui.add_space(8.0);
                    ui.label("Image adjustment");
                    let mut transform_changed = false;
                    ui.horizontal(|ui| {
                        ui.label("Horizontal scale");
                        transform_changed |= ui
                            .add(egui::Slider::new(&mut self.profile.lcd_image_scale_x, 0.25..=3.0).show_value(false))
                            .changed();
                        ui.monospace(format!("{:3.0}%", self.profile.lcd_image_scale_x * 100.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Vertical scale");
                        transform_changed |= ui
                            .add(egui::Slider::new(&mut self.profile.lcd_image_scale_y, 0.25..=3.0).show_value(false))
                            .changed();
                        ui.monospace(format!("{:3.0}%", self.profile.lcd_image_scale_y * 100.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Zoom");
                        transform_changed |= ui
                            .add(egui::Slider::new(&mut self.profile.lcd_image_zoom, 0.25..=4.0).show_value(false))
                            .changed();
                        ui.monospace(format!("{:3.0}%", self.profile.lcd_image_zoom * 100.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Anchor left / right");
                        transform_changed |= ui
                            .add(egui::Slider::new(&mut self.profile.lcd_image_anchor_x, -1.0..=1.0).show_value(false))
                            .changed();
                        ui.monospace(format!("{:+.0}%", self.profile.lcd_image_anchor_x * 100.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Anchor up / down");
                        transform_changed |= ui
                            .add(egui::Slider::new(&mut self.profile.lcd_image_anchor_y, -1.0..=1.0).show_value(false))
                            .changed();
                        ui.monospace(format!("{:+.0}%", self.profile.lcd_image_anchor_y * 100.0));
                    });
                    if ui.button("Reset image adjustment").clicked() {
                        self.profile.lcd_image_scale_x = 1.0;
                        self.profile.lcd_image_scale_y = 1.0;
                        self.profile.lcd_image_zoom = 1.0;
                        self.profile.lcd_image_anchor_x = 0.0;
                        self.profile.lcd_image_anchor_y = 0.0;
                        transform_changed = true;
                    }
                    if transform_changed {
                        let _ = config::save(&self.profile);
                        let _ = request(Request::SetLcdImageTransform {
                            scale_x: self.profile.lcd_image_scale_x,
                            scale_y: self.profile.lcd_image_scale_y,
                            zoom: self.profile.lcd_image_zoom,
                            anchor_x: self.profile.lcd_image_anchor_x,
                            anchor_y: self.profile.lcd_image_anchor_y,
                        });
                    }
                    if let Some(err) = &self.lcd_preview_error {
                        ui.colored_label(Color32::from_rgb(220, 110, 110), format!("Preview: {err}"));
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Refresh LCD").clicked() {
                        let _ = request(Request::LcdRefresh);
                    }
                    let lcd_ok = self.status.as_ref().map(|s| s.lcd_ok).unwrap_or(false);
                    ui.label(if lcd_ok {
                        "LCD output active"
                    } else {
                        "LCD output unavailable"
                    });
                });

                ui.separator();
                ui.heading("Macro Recorder");
                let rec = self.status.as_ref().map(|s| s.recording).unwrap_or(false);
                let rec_target = self.status.as_ref().and_then(|s| s.record_target.clone());
                if rec {
                    ui.colored_label(Color32::from_rgb(255, 90, 90), RichText::new("● RECORDING").strong());
                    match rec_target {
                        Some(ref target) => {
                            ui.label(format!("Target: {target}   /   M{}", self.profile.active_bank));
                            ui.small("Type the macro now. Press MR again to stop and save.");
                        }
                        None => {
                            ui.label(format!("M{}: waiting for destination", self.profile.active_bank));
                            ui.small("Press a G key, LCD key, thumb button, or stick direction to choose the macro destination.");
                        }
                    }
                } else {
                    ui.label("Recorder idle");
                    ui.small("Press MR, choose a destination, type the sequence, then press MR again.");
                }

                ui.separator();
                if !self.selected.is_empty() {
                    ui.heading(&self.selected);
                    ui.label("Binding");
                    ui.text_edit_singleline(&mut self.edit);
                    ui.horizontal(|ui| {
                        if ui.button("Save binding").clicked() {
                            self.profile
                                .bank_mut()
                                .bindings
                                .insert(self.selected.clone(), self.edit.clone());
                            let _ = config::save(&self.profile);
                            let _ = request(Request::SetBinding {
                                key: self.selected.clone(),
                                action: self.edit.clone(),
                            });
                        }
                        if ui.button("Clear").clicked() {
                            self.edit.clear();
                            self.profile.bank_mut().bindings.remove(&self.selected);
                            let _ = request(Request::SetBinding {
                                key: self.selected.clone(),
                                action: String::new(),
                            });
                        }
                    });
                    let count = self
                        .profile
                        .bank()
                        .macros
                        .get(&self.selected)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    if count > 0 {
                        ui.label(format!("Recorded macro: {count} events"));
                    }
                } else {
                    ui.label("Press or click a G/thumb/stick control to configure it.");
                }

                ui.separator();
                ui.label("Hardware MR workflow");
                ui.small(
                    "MR → destination control → type → MR. Key down/up timing is stored in the active M bank.",
                );
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.update_lcd_image_preview(ctx);
            let available = ui.available_rect_before_wrap();
            let body = available.shrink2(Vec2::new(12.0, 12.0));
            ui.painter().rect(
                body,
                24.0,
                Color32::from_gray(24),
                Stroke::new(2.0_f32, Color32::from_gray(75)),
            );

            // Keep one fixed logical hardware layout and scale the *whole* layout
            // uniformly to the available canvas. Nothing re-anchors independently
            // when the window changes size, so the thumb cluster cannot drift away.
            const DESIGN_W: f32 = 950.0;
            const DESIGN_H: f32 = 640.0;
            let scale = f32::min(body.width() / DESIGN_W, body.height() / DESIGN_H);
            let content_size = Vec2::new(DESIGN_W * scale, DESIGN_H * scale);
            let content_origin = body.center() - content_size * 0.5;

            let point = |x: f32, y: f32| {
                Pos2::new(content_origin.x + x * scale, content_origin.y + y * scale)
            };
            let size = |w: f32, h: f32| Vec2::new(w * scale, h * scale);
            let logical_rect = |x: f32, y: f32, w: f32, h: f32| {
                Rect::from_min_size(point(x, y), size(w, h))
            };

            let lcd_section = logical_rect(78.0, 28.0, 454.0, 206.0);
            let g_section = logical_rect(20.0, 262.0, 572.0, 322.0);
            let thumb_section = logical_rect(644.0, 130.0, 278.0, 220.0);
            let deadzone_section = logical_rect(644.0, 382.0, 278.0, 158.0);

            self.section_frame(ui, lcd_section, "Display & Mode Keys");
            self.section_frame(ui, g_section, "G Keys");
            self.section_frame(ui, thumb_section, "Thumb Controls");
            self.section_frame(ui, deadzone_section, "Joystick Dead Zone");

            let lcd_rect = Rect::from_min_size(
                Pos2::new(
                    lcd_section.center().x - 129.0 * scale,
                    lcd_section.top() + 28.0 * scale,
                ),
                size(258.0, 84.0),
            );
            let lcd_ink = Color32::from_rgb(
                (self.profile.color[0] as f32 * 0.10).round() as u8,
                (self.profile.color[1] as f32 * 0.10).round() as u8,
                (self.profile.color[2] as f32 * 0.10).round() as u8,
            );
            ui.painter().rect(
                lcd_rect,
                8.0 * scale,
                if self.profile.lcd_enabled {
                    Color32::from_rgb(self.profile.color[0], self.profile.color[1], self.profile.color[2])
                } else {
                    Color32::from_gray(45)
                },
                Stroke::new(2.0_f32 * scale, Color32::from_gray(80)),
            );
            if self.profile.lcd_page == 1 {
                for (row, line) in self.profile.lcd_lines.iter().enumerate() {
                    let y = lcd_rect.top() + (14.0 + row as f32 * 18.0) * scale;
                    let (pos, anchor) = match self.profile.lcd_align[row] {
                        LcdAlign::Left => (Pos2::new(lcd_rect.left() + 8.0 * scale, y), Align2::LEFT_CENTER),
                        LcdAlign::Center => (Pos2::new(lcd_rect.center().x, y), Align2::CENTER_CENTER),
                        LcdAlign::Right => (Pos2::new(lcd_rect.right() - 8.0 * scale, y), Align2::RIGHT_CENTER),
                    };
                    ui.painter().text(
                        pos,
                        anchor,
                        line,
                        FontId::monospace(13.0 * scale),
                        lcd_ink,
                    );
                }
            } else if self.profile.lcd_page == 3 {
                if let Some(texture) = &self.lcd_preview_texture {
                    let available_w = lcd_rect.width() - 16.0 * scale;
                    let available_h = lcd_rect.height() - 16.0 * scale;
                    let image_scale = f32::min(available_w / lcd::W as f32, available_h / lcd::H as f32);
                    let image_size = Vec2::new(lcd::W as f32 * image_scale, lcd::H as f32 * image_scale);
                    let image_rect = Rect::from_center_size(lcd_rect.center(), image_size);
                    ui.painter().image(
                        texture.id(),
                        image_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    let message = if self.profile.lcd_image_path.is_empty() {
                        "NO IMAGE"
                    } else if self.lcd_preview_error.is_some() {
                        "IMAGE ERROR"
                    } else {
                        "LOADING IMAGE"
                    };
                    ui.painter().text(
                        lcd_rect.center(),
                        Align2::CENTER_CENTER,
                        message,
                        FontId::monospace(14.0 * scale),
                        lcd_ink,
                    );
                }
            } else {
                let preview = match self.profile.lcd_page {
                    0 => format!("G13 NEXUS\n{}   M{}", self.profile.name, self.profile.active_bank),
                    _ => {
                        let (x, y) = self.status.as_ref().map(|s| (s.x, s.y)).unwrap_or((127, 127));
                        format!("INPUT MONITOR\nX:{x:03} Y:{y:03}")
                    }
                };
                ui.painter().text(
                    lcd_rect.center(),
                    Align2::CENTER_CENTER,
                    preview,
                    FontId::monospace(15.0 * scale),
                    lcd_ink,
                );
            }

            let function_w = 58.0 * scale;
            let function_gap = 10.0 * scale;
            let function_row_w = function_w * 4.0 + function_gap * 3.0;
            let function_x0 = lcd_section.center().x - function_row_w / 2.0;

            // The left round LCD key is programmable. The dedicated hardware
            // lighting button on the right is intentionally not shown or remappable;
            // it remains a fixed G13 backlight toggle.
            let lcd_round_y = lcd_section.top() + 146.0 * scale;
            let menu5_center = Pos2::new(function_x0 - 24.0 * scale, lcd_round_y);
            self.round_control_button(
                ui,
                menu5_center,
                13.0 * scale,
                "KEY_KBD_LCD_MENU5",
                "5",
            );

            for (i, name) in [
                "KEY_KBD_LCD_MENU1",
                "KEY_KBD_LCD_MENU2",
                "KEY_KBD_LCD_MENU3",
                "KEY_KBD_LCD_MENU4",
            ]
            .iter()
            .enumerate()
            {
                self.control_button(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(
                            function_x0 + i as f32 * (function_w + function_gap),
                            lcd_section.top() + 132.0 * scale,
                        ),
                        size(58.0, 28.0),
                    ),
                    name,
                    &format!("LCD {}", i + 1),
                );
            }

            for (i, (name, label)) in [
                ("KEY_MACRO_PRESET1", "M1"),
                ("KEY_MACRO_PRESET2", "M2"),
                ("KEY_MACRO_PRESET3", "M3"),
                ("KEY_MACRO_RECORD_START", "MR"),
            ]
            .iter()
            .enumerate()
            {
                self.control_button(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(
                            function_x0 + i as f32 * (function_w + function_gap),
                            lcd_section.top() + 170.0 * scale,
                        ),
                        size(58.0, 30.0),
                    ),
                    name,
                    label,
                );
            }

            let key_w = 66.0 * scale;
            let key_h = 46.0 * scale;
            let gap = 10.0 * scale;
            let field_center_x = g_section.center().x;
            let row_top = g_section.top() + 54.0 * scale;
            let row_step = 64.0 * scale;

            let rows: [(&[&str], f32); 4] = [
                (&["G1", "G2", "G3", "G4", "G5", "G6", "G7"], row_top),
                (&["G8", "G9", "G10", "G11", "G12", "G13", "G14"], row_top + row_step),
                (&["G15", "G16", "G17", "G18", "G19"], row_top + row_step * 2.0),
                (&["G20", "G21", "G22"], row_top + row_step * 3.0),
            ];
            for (keys, y) in rows {
                let row_width = keys.len() as f32 * key_w
                    + (keys.len().saturating_sub(1) as f32) * gap;
                let x0 = field_center_x - row_width / 2.0;
                for (i, key) in keys.iter().enumerate() {
                    self.control_button(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(x0 + i as f32 * (key_w + gap), y),
                            Vec2::new(key_w, key_h),
                        ),
                        key,
                        key,
                    );
                }
            }

            // Center the complete physical thumb-control footprint inside the
            // section. The left auxiliary button makes the geometry asymmetric
            // around the stick, so the stick itself sits slightly right of center.
            let center = Pos2::new(
                thumb_section.center().x + 58.0 * scale,
                thumb_section.center().y - 25.5 * scale,
            );

            // Compact physical thumb-control diagram.
            let left_btn_rect = Rect::from_center_size(
                center + Vec2::new(-130.0 * scale, 0.0),
                size(92.0, 38.0),
            );
            let bottom_btn_rect = Rect::from_center_size(
                center + Vec2::new(0.0, 94.0 * scale),
                size(104.0, 34.0),
            );
            self.control_button(ui, left_btn_rect, "BTN_BASE", "LEFT BTN");
            self.control_button(ui, bottom_btn_rect, "BTN_BASE2", "BOTTOM BTN");

            let up_rect = Rect::from_center_size(
                center + Vec2::new(0.0, -42.0 * scale),
                size(36.0, 36.0),
            );
            let left_rect = Rect::from_center_size(
                center + Vec2::new(-42.0 * scale, 0.0),
                size(36.0, 36.0),
            );
            let right_rect = Rect::from_center_size(
                center + Vec2::new(42.0 * scale, 0.0),
                size(36.0, 36.0),
            );
            let down_rect = Rect::from_center_size(
                center + Vec2::new(0.0, 42.0 * scale),
                size(36.0, 36.0),
            );

            let conn = Color32::from_gray(88);
            ui.painter().line_segment(
                [center + Vec2::new(0.0, -27.0 * scale), up_rect.center_bottom()],
                Stroke::new(1.4_f32 * scale, conn),
            );
            ui.painter().line_segment(
                [center + Vec2::new(-27.0 * scale, 0.0), Pos2::new(left_rect.right(), left_rect.center().y)],
                Stroke::new(1.4_f32 * scale, conn),
            );
            ui.painter().line_segment(
                [center + Vec2::new(27.0 * scale, 0.0), Pos2::new(right_rect.left(), right_rect.center().y)],
                Stroke::new(1.4_f32 * scale, conn),
            );
            ui.painter().line_segment(
                [center + Vec2::new(0.0, 27.0 * scale), down_rect.center_top()],
                Stroke::new(1.4_f32 * scale, conn),
            );

            self.overlay_zone(ui, up_rect, "ABS_Y-", "▲");
            self.overlay_zone(ui, left_rect, "ABS_X-", "◀");
            self.overlay_zone(ui, right_rect, "ABS_X+", "▶");
            self.overlay_zone(ui, down_rect, "ABS_Y+", "▼");

            let stick_rect = Rect::from_center_size(center, Vec2::splat(54.0 * scale));
            let stick_response = ui.interact(stick_rect, ui.id().with("BTN_THUMB"), Sense::click());
            let stick_live = self.status.as_ref()
                .map(|s| s.pressed.iter().any(|p| p == "BTN_THUMB"))
                .unwrap_or(false);
            let stick_fill = if stick_live {
                Color32::from_rgb(0, 190, 220)
            } else if self.selected == "BTN_THUMB" {
                Color32::from_rgb(70, 105, 145)
            } else {
                Color32::from_gray(50)
            };
            let stick_border = if self.is_record_target("BTN_THUMB") {
                Stroke::new(3.0_f32 * scale, Color32::from_rgb(255, 120, 55))
            } else {
                Stroke::new(2.0_f32 * scale, Color32::from_gray(110))
            };
            ui.painter().circle(
                center,
                27.0 * scale,
                Color32::from_gray(34),
                stick_border,
            );
            ui.painter().circle(
                center,
                21.0 * scale,
                stick_fill,
                Stroke::new(1.6_f32 * scale, Color32::from_gray(140)),
            );
            ui.painter().text(
                center + Vec2::new(0.0, -1.0 * scale),
                Align2::CENTER_CENTER,
                "CLICK",
                FontId::proportional(9.5 * scale),
                Color32::WHITE,
            );
            self.draw_macro_badge(ui, stick_rect, "BTN_THUMB");
            if stick_response.clicked() {
                self.select("BTN_THUMB");
            }

            let (x, y) = self.status.as_ref().map(|s| (s.x, s.y)).unwrap_or((127, 127));
            let dot = center + Vec2::new(
                (x as f32 - 127.0) / 127.0 * 12.0 * scale,
                (y as f32 - 127.0) / 127.0 * 12.0 * scale,
            );
            ui.painter()
                .circle_filled(dot, 3.6 * scale, Color32::from_rgb(0, 190, 220));

            // Live 2D dead-zone preview. The daemon uses independent X/Y
            // thresholds, so the inactive region is correctly shown as a square.
            let preview = Rect::from_min_size(
                Pos2::new(
                    deadzone_section.left() + 14.0 * scale,
                    deadzone_section.top() + 24.0 * scale,
                ),
                size(76.0, 76.0),
            );
            ui.painter().rect(
                preview,
                6.0 * scale,
                Color32::from_gray(28),
                Stroke::new(1.2_f32 * scale, Color32::from_gray(105)),
            );

            let status = self.status.as_ref();
            let sx = status.map(|s| s.x).unwrap_or(127) as f32;
            let sy = status.map(|s| s.y).unwrap_or(127) as f32;
            let cx = status.map(|s| s.center_x).unwrap_or(127) as f32;
            let cy = status.map(|s| s.center_y).unwrap_or(127) as f32;
            let dz = self.profile.deadzone as f32;

            let px = |raw: f32| preview.left() + (raw / 255.0) * preview.width();
            let py = |raw: f32| preview.top() + (raw / 255.0) * preview.height();

            let zone = Rect::from_min_max(
                Pos2::new(px((cx - dz).clamp(0.0, 255.0)), py((cy - dz).clamp(0.0, 255.0))),
                Pos2::new(px((cx + dz).clamp(0.0, 255.0)), py((cy + dz).clamp(0.0, 255.0))),
            );
            ui.painter().rect(
                zone,
                3.0 * scale,
                Color32::from_rgb(28, 88, 104),
                Stroke::new(2.0_f32 * scale, Color32::from_rgb(0, 220, 245)),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(px(cx), preview.top() + 2.0 * scale),
                    Pos2::new(px(cx), preview.bottom() - 2.0 * scale),
                ],
                Stroke::new(1.4_f32 * scale, Color32::WHITE),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(preview.left() + 2.0 * scale, py(cy)),
                    Pos2::new(preview.right() - 2.0 * scale, py(cy)),
                ],
                Stroke::new(1.4_f32 * scale, Color32::WHITE),
            );
            ui.painter().circle_filled(
                Pos2::new(px(sx), py(sy)),
                4.0 * scale,
                Color32::from_rgb(255, 205, 70),
            );

            // Keep a clear visual gutter between the preview and controls.
            let controls_x = deadzone_section.left() + 112.0 * scale;
            let controls_w = 148.0 * scale;
            let slider_track_center_x = controls_x + 56.0 * scale;

            // Give each slider/label pair more vertical breathing room.
            let dz_rect = Rect::from_min_size(
                Pos2::new(controls_x, deadzone_section.top() + 18.0 * scale),
                Vec2::new(controls_w, 18.0 * scale),
            );
            let deadzone_changed = ui.scope(|ui| {
                ui.visuals_mut().selection.bg_fill = Color32::from_rgb(0, 205, 230);
                ui.visuals_mut().widgets.active.bg_fill = Color32::from_rgb(0, 155, 180);
                ui.visuals_mut().widgets.hovered.bg_fill = Color32::from_rgb(50, 115, 135);
                ui.put(
                    dz_rect,
                    egui::Slider::new(&mut self.profile.deadzone, 8..=100)
                        .show_value(true),
                )
                .on_hover_text(
                    "Raw counts away from the calibrated center before a direction activates."
                )
                .changed()
            }).inner;
            ui.painter().text(
                Pos2::new(slider_track_center_x, deadzone_section.top() + 42.0 * scale),
                Align2::CENTER_CENTER,
                "Dead zone",
                FontId::proportional(9.5 * scale),
                Color32::from_gray(190),
            );

            if deadzone_changed {
                let _ = request(Request::SetDeadzone { value: self.profile.deadzone });
            }

            let mut center_x_edit = status.map(|s| s.center_x).unwrap_or(127);
            let mut center_y_edit = status.map(|s| s.center_y).unwrap_or(127);

            let x_rect = Rect::from_min_size(
                Pos2::new(controls_x, deadzone_section.top() + 50.0 * scale),
                Vec2::new(controls_w, 18.0 * scale),
            );
            let x_changed = ui.put(
                x_rect,
                egui::Slider::new(&mut center_x_edit, 0..=255)
                    .show_value(true),
            )
            .on_hover_text("Move the vertical center line left/right.")
            .changed();
            ui.painter().text(
                Pos2::new(slider_track_center_x, deadzone_section.top() + 74.0 * scale),
                Align2::CENTER_CENTER,
                "Center X",
                FontId::proportional(9.5 * scale),
                Color32::from_gray(190),
            );

            let y_rect = Rect::from_min_size(
                Pos2::new(controls_x, deadzone_section.top() + 82.0 * scale),
                Vec2::new(controls_w, 18.0 * scale),
            );
            let y_changed = ui.put(
                y_rect,
                egui::Slider::new(&mut center_y_edit, 0..=255)
                    .show_value(true),
            )
            .on_hover_text("Move the horizontal center line up/down.")
            .changed();
            ui.painter().text(
                Pos2::new(slider_track_center_x, deadzone_section.top() + 106.0 * scale),
                Align2::CENTER_CENTER,
                "Center Y",
                FontId::proportional(9.5 * scale),
                Color32::from_gray(190),
            );

            if x_changed || y_changed {
                self.profile.joystick_center_x = Some(center_x_edit);
                self.profile.joystick_center_y = Some(center_y_edit);
                let _ = request(Request::SetJoystickCenter {
                    x: Some(center_x_edit),
                    y: Some(center_y_edit),
                });
            }

            // Equal-size one-word calibration actions.
            let button_gap = 8.0 * scale;
            let button_w = (controls_w - button_gap) / 2.0;
            let button_y = deadzone_section.top() + 119.0 * scale;

            let use_current_rect = Rect::from_min_size(
                Pos2::new(controls_x, button_y),
                Vec2::new(button_w, 22.0 * scale),
            );
            if ui.put(use_current_rect, egui::Button::new("Capture")).clicked() {
                let raw_x = status.map(|s| s.x).unwrap_or(127);
                let raw_y = status.map(|s| s.y).unwrap_or(127);
                self.profile.joystick_center_x = Some(raw_x);
                self.profile.joystick_center_y = Some(raw_y);
                let _ = request(Request::SetJoystickCenter {
                    x: Some(raw_x),
                    y: Some(raw_y),
                });
            }

            let reset_rect = Rect::from_min_size(
                Pos2::new(controls_x + button_w + button_gap, button_y),
                Vec2::new(button_w, 22.0 * scale),
            );
            if ui.put(reset_rect, egui::Button::new("Reset")).clicked() {
                self.profile.joystick_center_x = None;
                self.profile.joystick_center_y = None;
                let _ = request(Request::SetJoystickCenter { x: None, y: None });
            }

            // Legend sits immediately below the crosshair preview and is
            // horizontally centered against that box.
            let legend_y = preview.bottom() + 14.0 * scale;
            let legend_center = preview.center().x;
            let legend_left = legend_center - 30.0 * scale;
            ui.painter().circle_filled(
                Pos2::new(legend_left, legend_y),
                3.0 * scale,
                Color32::from_rgb(255, 205, 70),
            );
            ui.painter().text(
                Pos2::new(legend_left + 6.0 * scale, legend_y),
                Align2::LEFT_CENTER,
                "raw",
                FontId::proportional(8.5 * scale),
                Color32::from_gray(185),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(legend_left + 30.0 * scale, legend_y),
                    Pos2::new(legend_left + 38.0 * scale, legend_y),
                ],
                Stroke::new(1.5_f32 * scale, Color32::WHITE),
            );
            ui.painter().text(
                Pos2::new(legend_left + 42.0 * scale, legend_y),
                Align2::LEFT_CENTER,
                "center",
                FontId::proportional(8.5 * scale),
                Color32::from_gray(185),
            );

            // Main-canvas description: centered, anchored near the bottom, with
            // explicit padding from the outer frame.
            ui.painter().text(
                point(DESIGN_W / 2.0, DESIGN_H - 2.0),
                Align2::CENTER_BOTTOM,
                "Select a control or press it on the G13 to edit its binding. The thumb diagram shows physical controls and directional mappings separately.",
                FontId::proportional(12.0 * scale),
                Color32::from_gray(145),
            );
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 800.0])
            .with_min_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "G13 Nexus",
        options,
        Box::new(|_| Box::<App>::default()),
    )
}
