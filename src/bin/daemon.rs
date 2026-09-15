use anyhow::{Context, Result};
use g13_nexus::{
    config::{self, MacroStep},
    input::{self, DeviceKind, InputDevice},
    ipc::{self, Reply, Request},
    lcd, leds,
    uinput::{keycode, keyname, VirtualKeyboard},
};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Shared {
    reply: Reply,
    status_cache: Arc<Mutex<Reply>>,
    profile: config::Profile,
    detected_center_x: u8,
    detected_center_y: u8,
    rec_last: Option<Instant>,
    rec_down: HashSet<String>,
    rec_recent: HashMap<(String, bool), Instant>,
}

fn emit_action(keyboard: &VirtualKeyboard, action: &str, down: bool) {
    let parts: Vec<u16> = action.split('+').filter_map(keycode).collect();
    if down {
        for code in &parts { let _ = keyboard.key(*code, 1); }
    } else {
        for code in parts.iter().rev() { let _ = keyboard.key(*code, 0); }
    }
}

fn play_macro(keyboard: &VirtualKeyboard, steps: &[MacroStep]) {
    let steps = steps.to_vec();
    let Ok(keyboard) = keyboard.try_clone() else { return; };
    thread::spawn(move || {
        let mut held = HashSet::new();
        for step in steps {
            thread::sleep(Duration::from_millis(step.delay_ms.min(5_000)));
            if let Some(code) = keycode(&step.key) {
                let _ = keyboard.key(code, if step.down { 1 } else { 0 });
                if step.down { held.insert(code); } else { held.remove(&code); }
            }
        }
        // Never leave a virtual modifier/key held if a recording ended mid-key.
        for code in held {
            let _ = keyboard.key(code, 0);
        }
    });
}

fn action_for(shared: &Shared, name: &str) -> (Option<String>, Option<Vec<MacroStep>>) {
    let bank = shared.profile.bank();
    (bank.bindings.get(name).cloned(), bank.macros.get(name).cloned())
}

fn set_control(
    shared: &Arc<Mutex<Shared>>,
    keyboard: &VirtualKeyboard,
    pressed: &mut HashSet<String>,
    name: &str,
    down: bool,
) {
    let changed = if down { pressed.insert(name.to_owned()) } else { pressed.remove(name) };
    if !changed { return; }

    // Any programmable physical control can become the MR destination,
    // including joystick directions generated from ABS_X/ABS_Y.
    if down && arm_record_target(shared, name) { return; }

    let (action, macro_steps) = {
        let sh = shared.lock().unwrap();
        action_for(&sh, name)
    };

    if let Some(steps) = macro_steps {
        if down { play_macro(keyboard, &steps); }
    } else if let Some(action) = action {
        emit_action(keyboard, &action, down);
    }
}

fn update_directions(
    shared: &Arc<Mutex<Shared>>,
    keyboard: &VirtualKeyboard,
    pressed: &mut HashSet<String>,
    x: u8,
    y: u8,
) {
    let (deadzone, center_x, center_y) = {
        let sh = shared.lock().unwrap();
        let (cx, cy) = effective_center(&sh);
        (sh.profile.deadzone as i16, cx, cy)
    };
    // Once a direction is active, use a smaller release threshold so normal
    // analog jitter cannot rapidly toggle the virtual key at the boundary.
    let hysteresis = (deadzone / 4).clamp(4, 12);
    let release_zone = (deadzone - hysteresis).max(1);

    let x = x as i16;
    let y = y as i16;
    let cx = center_x as i16;
    let cy = center_y as i16;

    let x_neg = if pressed.contains("ABS_X-") {
        x < cx - release_zone
    } else {
        x < cx - deadzone
    };
    let x_pos = if pressed.contains("ABS_X+") {
        x > cx + release_zone
    } else {
        x > cx + deadzone
    };
    let y_neg = if pressed.contains("ABS_Y-") {
        y < cy - release_zone
    } else {
        y < cy - deadzone
    };
    let y_pos = if pressed.contains("ABS_Y+") {
        y > cy + release_zone
    } else {
        y > cy + deadzone
    };

    for (name, active) in [
        ("ABS_X-", x_neg),
        ("ABS_X+", x_pos),
        ("ABS_Y-", y_neg),
        ("ABS_Y+", y_pos),
    ] {
        set_control(shared, keyboard, pressed, name, active);
    }
}

fn refresh_lcd(sh: &mut Shared) {
    sh.reply.lcd_ok = lcd::show(
        &sh.profile.name,
        sh.profile.active_bank,
        sh.reply.recording,
        sh.profile.lcd_enabled,
        sh.profile.lcd_page,
        &sh.profile.lcd_lines,
        &sh.profile.lcd_align,
        &sh.profile.lcd_image_path,
        sh.profile.lcd_image_scale_x,
        sh.profile.lcd_image_scale_y,
        sh.profile.lcd_image_zoom,
        sh.profile.lcd_image_anchor_x,
        sh.profile.lcd_image_anchor_y,
        sh.reply.x,
        sh.reply.y,
        &sh.reply.pressed,
    )
    .is_ok();
}

fn refresh_lcd_shared(shared: &Arc<Mutex<Shared>>) {
    let (profile, reply) = {
        let sh = shared.lock().unwrap();
        (sh.profile.clone(), sh.reply.clone())
    };
    let ok = lcd::show(
        &profile.name,
        profile.active_bank,
        reply.recording,
        profile.lcd_enabled,
        profile.lcd_page,
        &profile.lcd_lines,
        &profile.lcd_align,
        &profile.lcd_image_path,
        profile.lcd_image_scale_x,
        profile.lcd_image_scale_y,
        profile.lcd_image_zoom,
        profile.lcd_image_anchor_x,
        profile.lcd_image_anchor_y,
        reply.x,
        reply.y,
        &reply.pressed,
    ).is_ok();
    shared.lock().unwrap().reply.lcd_ok = ok;
}

fn has_mapping(shared: &Arc<Mutex<Shared>>, name: &str) -> bool {
    let sh = shared.lock().unwrap();
    let bank = sh.profile.bank();
    bank.bindings.contains_key(name) || bank.macros.contains_key(name)
}

fn apply_visuals(sh: &mut Shared) {
    let _ = leds::set_rgb(sh.profile.color);
    let _ = leds::set_bank(sh.profile.active_bank);
    let _ = leds::set_record(sh.reply.recording);
    refresh_lcd(sh);
}


fn effective_center(sh: &Shared) -> (u8, u8) {
    (
        sh.profile.joystick_center_x.unwrap_or(sh.detected_center_x),
        sh.profile.joystick_center_y.unwrap_or(sh.detected_center_y),
    )
}

fn sync_profile_reply(sh: &mut Shared) {
    sh.reply.active_bank = sh.profile.active_bank;
    sh.reply.lcd_enabled = sh.profile.lcd_enabled;
    sh.reply.lcd_page = sh.profile.lcd_page;
    sh.reply.profile_name = sh.profile.name.clone();
    sh.reply.macro_targets = sh.profile.bank().macros.iter()
        .filter_map(|(key, steps)| if steps.is_empty() { None } else { Some(key.clone()) })
        .collect();
    let (center_x, center_y) = effective_center(sh);
    sh.reply.center_x = center_x;
    sh.reply.center_y = center_y;
}

fn cache_status(sh: &Shared) {
    if let Ok(mut cache) = sh.status_cache.lock() {
        *cache = sh.reply.clone();
    }
}

fn refresh_profile_list(sh: &mut Shared) {
    sh.reply.profiles = config::list_profiles();
}
fn publish(
    shared: &Arc<Mutex<Shared>>,
    pressed: &HashSet<String>,
    x: u8,
    y: u8,
    connected: bool,
    message: &str,
) {
    let mut pressed: Vec<_> = pressed.iter().cloned().collect();
    pressed.sort();

    let mut sh = shared.lock().unwrap();
    sh.reply.ok = connected;
    sh.reply.message = message.to_owned();
    sh.reply.connected = connected;
    sh.reply.x = x;
    sh.reply.y = y;
    sh.reply.pressed = pressed;
    sync_profile_reply(&mut sh);
    cache_status(&sh);
}

fn socket_thread(shared: Arc<Mutex<Shared>>, status_cache: Arc<Mutex<Reply>>) {
    let path = ipc::socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind G13 Nexus IPC socket");

    // Status is the hot path. Parse it directly on the listener thread and
    // answer from the independent cache without creating a worker thread.
    // Only infrequent mutating requests get their own worker.
    for mut stream in listener.incoming().flatten() {
        let mut line = String::new();
        if BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).is_err() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            continue;
        };

        if matches!(request, Request::Status) {
            if let Ok(reply) = status_cache.lock().map(|reply| reply.clone()) {
                let _ = writeln!(stream, "{}", serde_json::to_string(&reply).unwrap());
            }
            continue;
        }

        let shared = shared.clone();
        let status_cache = status_cache.clone();
        thread::spawn(move || handle_request(stream, request, &shared, &status_cache));
    }
}

fn record_key_event(sh: &mut Shared, key: String, down: bool) {
    if !sh.reply.recording || sh.reply.record_target.is_none() || keycode(&key).is_none() {
        return;
    }

    // The secure GUI recorder path and raw evdev path may both see the same
    // physical keystroke. Collapse only near-simultaneous identical edges; the
    // normal held-key state check below still protects against repeats.
    let now = Instant::now();
    let signature = (key.clone(), down);
    if sh.rec_recent.get(&signature)
        .map(|last| now.duration_since(*last) < Duration::from_millis(30))
        .unwrap_or(false)
    {
        return;
    }
    sh.rec_recent.insert(signature, now);
    sh.rec_recent.retain(|_, seen| now.duration_since(*seen) < Duration::from_secs(2));

    let changed = if down { sh.rec_down.insert(key.clone()) } else { sh.rec_down.remove(&key) };
    if !changed { return; }

    let delay_ms = sh.rec_last
        .map(|last| now.duration_since(last).as_millis() as u64)
        .unwrap_or(0)
        .min(5_000);
    sh.rec_last = Some(now);

    if let Some(target) = sh.reply.record_target.clone() {
        let steps = sh.profile.bank_mut().macros.entry(target).or_default();
        if steps.len() < 4096 {
            let step = MacroStep { key, down, delay_ms };
            steps.push(step.clone());
            sh.reply.record_preview.push(step);
            sync_profile_reply(sh);
            cache_status(sh);
        }
    }
}

fn keyboard_recorder_thread(shared: Arc<Mutex<Shared>>) {
    let mut taps: Vec<input::InputTap> = Vec::new();
    let mut active_last = false;
    let mut last_scan = Instant::now() - Duration::from_secs(10);

    loop {
        let active = {
            let sh = shared.lock().unwrap();
            sh.reply.recording && sh.reply.record_target.is_some()
        };

        if !active {
            if active_last { taps.clear(); }
            active_last = false;
            thread::sleep(Duration::from_millis(20));
            continue;
        }

        if !active_last || last_scan.elapsed() >= Duration::from_secs(2) {
            taps.clear();
            if let Ok(nodes) = input::keyboard_event_nodes() {
                for path in nodes {
                    if let Ok(tap) = input::InputTap::open(path) {
                        taps.push(tap);
                    }
                }
            }
            last_scan = Instant::now();
        }
        active_last = true;

        let mut disconnected = false;
        for tap in &mut taps {
            match tap.read_events() {
                Ok(events) => {
                    for event in events {
                        if event.type_ != input::EV_KEY || event.value == 2 { continue; }
                        let Some(name) = keyname(event.code) else { continue; };
                        let mut sh = shared.lock().unwrap();
                        record_key_event(&mut sh, name.to_owned(), event.value != 0);
                    }
                }
                Err(_) => disconnected = true,
            }
        }
        if disconnected { last_scan = Instant::now() - Duration::from_secs(10); }
        thread::sleep(Duration::from_millis(4));
    }
}

fn handle_request(
    mut stream: UnixStream,
    request: Request,
    shared: &Arc<Mutex<Shared>>,
    _status_cache: &Arc<Mutex<Reply>>,
) {
    let mut sh = shared.lock().unwrap();
    match request {
        Request::Status => unreachable!(),
        Request::Reload => {
            sh.profile = config::load();
            apply_visuals(&mut sh);
        }
        Request::LoadProfile { name } => {
            if let Some(profile) = config::load_named(&name) {
                let _ = config::set_active(&name);
                sh.profile = profile;
                sh.reply.message = format!("profile loaded: {}", sh.profile.name);
                    refresh_profile_list(&mut sh);
                apply_visuals(&mut sh);
            } else {
                sh.reply.message = format!("profile not found: {name}");
            }
        }
        Request::SaveProfileAs { name } => {
            match config::save_as(&name, &sh.profile) {
                Ok(profile) => {
                    sh.profile = profile;
                    sh.reply.message = format!("profile saved: {}", sh.profile.name);
                    refresh_profile_list(&mut sh);
                    apply_visuals(&mut sh);
                }
                Err(error) => sh.reply.message = format!("save profile failed: {error}"),
            }
        }
        Request::RenameProfile { name } => {
            match config::rename_active(&name, &sh.profile) {
                Ok(profile) => {
                    sh.profile = profile;
                    sh.reply.message = format!("profile renamed: {}", sh.profile.name);
                    refresh_profile_list(&mut sh);
                    apply_visuals(&mut sh);
                }
                Err(error) => sh.reply.message = format!("rename profile failed: {error}"),
            }
        }
        Request::DeleteProfile => {
            match config::delete_active() {
                Ok(profile) => {
                    sh.profile = profile;
                    sh.reply.message = format!("profile loaded: {}", sh.profile.name);
                    refresh_profile_list(&mut sh);
                    apply_visuals(&mut sh);
                }
                Err(error) => sh.reply.message = format!("delete profile failed: {error}"),
            }
        }
        Request::SetColor { rgb } => {
            sh.profile.color = rgb;
            let _ = config::save(&sh.profile);
            let _ = leds::set_rgb(rgb);
        }
        Request::SetDeadzone { value } => {
            sh.profile.deadzone = value;
            let _ = config::save(&sh.profile);
        }
        Request::SetJoystickCenter { x, y } => {
            sh.profile.joystick_center_x = x;
            sh.profile.joystick_center_y = y;
            sync_profile_reply(&mut sh);
            cache_status(&sh);
            let _ = config::save(&sh.profile);
        }
        Request::SetBinding { key, action } => {
            let bank = sh.profile.bank_mut();
            bank.macros.remove(&key);
            if action.trim().is_empty() { bank.bindings.remove(&key); }
            else { bank.bindings.insert(key, action); }
            let _ = config::save(&sh.profile);
        }
        Request::SetBank { bank } => {
            if (1..=3).contains(&bank) && sh.reply.record_target.is_none() {
                sh.profile.active_bank = bank;
                let _ = config::save(&sh.profile);
                apply_visuals(&mut sh);
            }
        }
        Request::SetLcdEnabled { enabled } => {
            sh.profile.lcd_enabled = enabled;
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::SetLcdPage { page } => {
            sh.profile.lcd_page = page % 4;
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::SetLcdLines { lines } => {
            sh.profile.lcd_lines = lines;
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::SetLcdAlign { align } => {
            sh.profile.lcd_align = align;
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::SetLcdImage { path } => {
            sh.profile.lcd_image_path = path;
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::SetLcdImageTransform { scale_x, scale_y, zoom, anchor_x, anchor_y } => {
            sh.profile.lcd_image_scale_x = scale_x.clamp(0.25, 3.0);
            sh.profile.lcd_image_scale_y = scale_y.clamp(0.25, 3.0);
            sh.profile.lcd_image_zoom = zoom.clamp(0.25, 4.0);
            sh.profile.lcd_image_anchor_x = anchor_x.clamp(-1.0, 1.0);
            sh.profile.lcd_image_anchor_y = anchor_y.clamp(-1.0, 1.0);
            let _ = config::save(&sh.profile);
            refresh_lcd(&mut sh);
        }
        Request::LcdRefresh => refresh_lcd(&mut sh),
        Request::RecordToggle => {
            if sh.reply.recording { finish_recording(&mut sh, true); } else { start_recording(&mut sh); }
            sync_profile_reply(&mut sh);
            apply_visuals(&mut sh);
        }
        Request::RecordTarget { key } => {
            if sh.reply.recording && sh.reply.record_target.is_none() && recordable_control(&key) {
                sh.reply.record_target = Some(key.clone());
                sh.reply.record_preview.clear();
                sh.profile.bank_mut().bindings.remove(&key);
                sh.profile.bank_mut().macros.insert(key, Vec::new());
                sh.rec_last = None;
                sh.rec_down.clear();
                sh.rec_recent.clear();
                let _ = config::save(&sh.profile);
                refresh_lcd(&mut sh);
            }
        }
        Request::RecordEvent { key, down } => record_key_event(&mut sh, key, down),
    }

    sync_profile_reply(&mut sh);
    cache_status(&sh);
    let _ = writeln!(stream, "{}", serde_json::to_string(&sh.reply).unwrap());
}

fn recordable_control(name: &str) -> bool {
    if matches!(name, "BTN_BASE" | "BTN_BASE2" | "BTN_THUMB" | "ABS_X-" | "ABS_X+" | "ABS_Y-" | "ABS_Y+") {
        return true;
    }
    if let Some(number) = name.strip_prefix('G').and_then(|value| value.parse::<u8>().ok()) {
        return (1..=22).contains(&number);
    }
    matches!(name,
        "KEY_KBD_LCD_MENU1" | "KEY_KBD_LCD_MENU2" | "KEY_KBD_LCD_MENU3" |
        "KEY_KBD_LCD_MENU4" | "KEY_KBD_LCD_MENU5"
    )
}

fn finish_recording(sh: &mut Shared, persist: bool) {
    if let Some(target) = sh.reply.record_target.clone() {
        let mut held: Vec<_> = sh.rec_down.drain().collect();
        held.sort();
        if !held.is_empty() {
            let first_delay = sh.rec_last
                .map(|last| Instant::now().duration_since(last).as_millis() as u64)
                .unwrap_or(0)
                .min(5_000);
            let steps = sh.profile.bank_mut().macros.entry(target).or_default();
            for (index, key) in held.into_iter().enumerate() {
                steps.push(MacroStep { key, down: false, delay_ms: if index == 0 { first_delay } else { 0 } });
            }
        }
        if persist {
            let _ = config::save(&sh.profile);
        }
    }

    // A recording session is fully disposable state. Clear every field here,
    // even if a previous path left a partial target behind.
    sh.reply.recording = false;
    sh.reply.record_target = None;
    sh.reply.record_preview.clear();
    sh.rec_last = None;
    sh.rec_down.clear();
    sh.rec_recent.clear();
    sync_profile_reply(sh);
    cache_status(sh);
}

fn start_recording(sh: &mut Shared) {
    // Starting a new recording always starts from a clean session. This makes
    // MR re-arm reliable even after a completed or interrupted recording.
    sh.reply.recording = true;
    sh.reply.record_target = None;
    sh.reply.record_preview.clear();
    sh.rec_last = None;
    sh.rec_down.clear();
    sh.rec_recent.clear();
    sync_profile_reply(sh);
    cache_status(sh);
}

fn switch_bank(shared: &Arc<Mutex<Shared>>, bank: u8) {
    // Update the daemon's authoritative in-memory state first and release the
    // mutex immediately. GUI Status requests must never wait on disk writes,
    // sysfs LED writes, or hidraw LCD rendering.
    let profile_to_save = {
        let mut sh = shared.lock().unwrap();
        if sh.reply.recording && sh.reply.record_target.is_some() { return; }
        if sh.profile.active_bank == bank {
            sync_profile_reply(&mut sh);
            cache_status(&sh);
            None
        } else {
            sh.profile.active_bank = bank;
            sync_profile_reply(&mut sh);
            cache_status(&sh);
            Some(sh.profile.clone())
        }
    };

    // Mode selection itself must remain gaming-input fast. LED update is a
    // tiny sysfs write and happens after releasing Shared. Profile persistence
    // is background work. Do not render the LCD on every M-key tap; repeated
    // hidraw writes were creating a backlog during rapid bank switching.
    let _ = leds::set_bank(bank);
    if let Some(profile) = profile_to_save {
        thread::spawn(move || {
            let _ = config::save(&profile);
        });
    }
}

fn toggle_recording(shared: &Arc<Mutex<Shared>>) {
    let (recording, profile_to_save) = {
        let mut sh = shared.lock().unwrap();

        let profile_to_save = if sh.reply.recording {
            finish_recording(&mut sh, false);
            Some(sh.profile.clone())
        } else {
            start_recording(&mut sh);
            None
        };

        // The state returned to IPC is final before any slow work is queued.
        sync_profile_reply(&mut sh);
        cache_status(&sh);
        (sh.reply.recording, profile_to_save)
    };

    // MR state must react immediately. Update the dedicated MR LED after the
    // state transition and persist a finished recording in the background.
    // LCD rendering is deliberately not tied to the button edge.
    let _ = leds::set_record(recording);
    if let Some(profile) = profile_to_save {
        thread::spawn(move || {
            let _ = config::save(&profile);
        });
    }
}

fn lcd_button(shared: &Arc<Mutex<Shared>>, name: &str) {
    {
        let mut sh = shared.lock().unwrap();
        match name {
            "KEY_KBD_LCD_MENU1" => sh.profile.lcd_page = (sh.profile.lcd_page + 3) % 4,
            "KEY_KBD_LCD_MENU2" => sh.profile.lcd_page = (sh.profile.lcd_page + 1) % 4,
            "KEY_KBD_LCD_MENU3" => sh.profile.lcd_enabled = !sh.profile.lcd_enabled,
            "KEY_KBD_LCD_MENU4" => {
                sh.profile.lcd_enabled = true;
                sh.profile.lcd_page = 0;
            }
            // The kernel G13 map identifies MENU5 as the separate round
            // "Next page" key to the left of the four rectangular LCD keys.
            "KEY_KBD_LCD_MENU5" => sh.profile.lcd_page = (sh.profile.lcd_page + 1) % 4,
            _ => return,
        }
        let _ = config::save(&sh.profile);
        sync_profile_reply(&mut sh);
    }
    refresh_lcd_shared(shared);
}

fn arm_record_target(shared: &Arc<Mutex<Shared>>, name: &str) -> bool {
    let mut sh = shared.lock().unwrap();
    if !sh.reply.recording || sh.reply.record_target.is_some() || !recordable_control(name) { return false; }

    sh.reply.record_target = Some(name.to_owned());
    sh.reply.record_preview.clear();
    sh.profile.bank_mut().bindings.remove(name);
    sh.profile.bank_mut().macros.insert(name.to_owned(), Vec::new());
    sh.rec_last = None;
    sh.rec_down.clear();
    sh.rec_recent.clear();
    sync_profile_reply(&mut sh);
    cache_status(&sh);

    // The completed macro is persisted when MR stops. Avoid synchronous disk
    // and LCD work while arming the target so the recorder can be re-used
    // immediately for back-to-back recordings.
    true
}

fn main() -> Result<()> {
    let mut profile = config::load();
    profile.normalize();
    config::save(&profile).ok();

    let initial_reply = Reply {
        ok: true,
        message: "starting".into(),
        connected: false,
        x: 127,
        y: 127,
        center_x: 127,
        center_y: 127,
        pressed: Vec::new(),
        active_bank: profile.active_bank,
        recording: false,
        record_target: None,
        record_preview: Vec::new(),
        macro_targets: Vec::new(),
        lcd_ok: false,
        lcd_enabled: profile.lcd_enabled,
        lcd_page: profile.lcd_page,
        profile_name: profile.name.clone(),
        profiles: config::list_profiles(),
    };
    let status_cache = Arc::new(Mutex::new(initial_reply.clone()));

    let shared = Arc::new(Mutex::new(Shared {
        reply: initial_reply,
        status_cache: status_cache.clone(),
        profile,
        detected_center_x: 127,
        detected_center_y: 127,
        rec_last: None,
        rec_down: HashSet::new(),
        rec_recent: HashMap::new(),
    }));

    {
        let shared = shared.clone();
        let status_cache = status_cache.clone();
        thread::spawn(move || socket_thread(shared, status_cache));
    }
    {
        let shared = shared.clone();
        thread::spawn(move || keyboard_recorder_thread(shared));
    }

    let keyboard = VirtualKeyboard::new()
        .context("create virtual keyboard; check /dev/uinput permissions")?;

    loop {
        let (keypad_path, stick_path) = match input::discover() {
            Ok(paths) => paths,
            Err(error) => {
                publish(&shared, &HashSet::new(), 127, 127, false, &error.to_string());
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        let keypad_path_saved = keypad_path.clone();
        let stick_path_saved = stick_path.clone();
        let mut keypad = match InputDevice::open(DeviceKind::Keypad, keypad_path) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("{error:#}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let mut stick = match InputDevice::open(DeviceKind::Thumbstick, stick_path) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("{error:#}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        // Keep every evdev node belonging to USB 046d:c21c exclusively grabbed.
        // This prevents desktop environments from also interpreting special G13
        // keycodes such as KEY_LIGHTS_TOGGLE as laptop/display brightness keys.
        let mut auxiliary_grabs = Vec::new();
        let all_g13_nodes = input::g13_event_nodes().unwrap_or_default();
        let mut auxiliary_grab_failed = false;
        for path in all_g13_nodes {
            if path == keypad_path_saved || path == stick_path_saved {
                continue;
            }
            match InputDevice::open(DeviceKind::Keypad, path.clone()) {
                Ok(mut device) => match device.grab_exclusive() {
                    Ok(()) => auxiliary_grabs.push(device),
                    Err(error) => {
                        eprintln!("G13 auxiliary exclusive grab unavailable for {}: {error:#}", path.display());
                        auxiliary_grab_failed = true;
                    }
                },
                Err(error) => {
                    eprintln!("G13 auxiliary input open failed for {}: {error:#}", path.display());
                    auxiliary_grab_failed = true;
                }
            }
        }

        // Capture the kernel-reported resting position rather than assuming 127/127.
        // The real G13 commonly rests a few counts off nominal center.
        let center_x = stick.abs_value(input::ABS_X).unwrap_or(127).clamp(0, 255) as u8;
        let center_y = stick.abs_value(input::ABS_Y).unwrap_or(127).clamp(0, 255) as u8;
        {
            let mut sh = shared.lock().unwrap();
            sh.detected_center_x = center_x;
            sh.detected_center_y = center_y;
            sync_profile_reply(&mut sh);
            cache_status(&sh);
        }

        // Keep the physical G13 events from also leaking to the desktop while
        // Nexus emits the configured uinput actions. A failed grab is non-fatal
        // so access-policy problems do not make the device unusable.
        let keypad_grabbed = match keypad.grab_exclusive() {
            Ok(()) => true,
            Err(error) => {
                eprintln!("G13 keypad exclusive grab unavailable: {error:#}");
                false
            }
        };
        let stick_grabbed = match stick.grab_exclusive() {
            Ok(()) => true,
            Err(error) => {
                eprintln!("G13 thumbstick exclusive grab unavailable: {error:#}");
                false
            }
        };

        if !keypad_grabbed || !stick_grabbed || auxiliary_grab_failed {
            eprintln!("G13 exclusive capture incomplete; retrying instead of leaking raw special-key events to the desktop");
            thread::sleep(Duration::from_secs(2));
            continue;
        }

        eprintln!(
            "G13 connected through hid-lg-g15 evdev; center={center_x},{center_y}; exclusive=true; auxiliary_grabs={}",
            auxiliary_grabs.len()
        );
        {
            let mut sh = shared.lock().unwrap();
            apply_visuals(&mut sh);
        }

        let mut pressed = HashSet::new();
        // M/LCD function keys can produce down+up in one evdev batch. Keep a
        // visual-only pulse separate from logical pressed state, so rapid taps
        // remain responsive without suppressing remapped key actions.
        let mut function_visual_until: HashMap<String, Instant> = HashMap::new();
        let (mut x, mut y) = (center_x, center_y);
        publish(&shared, &pressed, x, y, true, "connected via hid-lg-g15");

        'connected: loop {
            let mut had_input = false;
            for device in [&mut keypad, &mut stick] {
                let events = match device.read_events() {
                    Ok(events) => events,
                    Err(error) => {
                        eprintln!("G13 input read error: {error:#}");
                        break 'connected;
                    }
                };

                if !events.is_empty() { had_input = true; }

                for event in events {
                    if event.type_ == input::EV_KEY && event.value != 2 {
                        let Some(name) = input::key_control(event.code) else { continue; };
                        let down = event.value != 0;

                        // M1-M3 and MR remain dedicated hardware mode controls.
                        // Their visual pulse is independent of logical pressed state.
                        let dedicated_function = matches!(name.as_str(),
                            "KEY_MACRO_PRESET1" | "KEY_MACRO_PRESET2" |
                            "KEY_MACRO_PRESET3" | "KEY_MACRO_RECORD_START"
                        );

                        if dedicated_function {
                            if down {
                                function_visual_until.insert(
                                    name.clone(),
                                    Instant::now() + Duration::from_millis(220),
                                );
                                let mut visible = pressed.clone();
                                visible.insert(name.clone());
                                publish(&shared, &visible, x, y, true, "connected via hid-lg-g15");
                                match name.as_str() {
                                    "KEY_MACRO_PRESET1" => switch_bank(&shared, 1),
                                    "KEY_MACRO_PRESET2" => switch_bank(&shared, 2),
                                    "KEY_MACRO_PRESET3" => switch_bank(&shared, 3),
                                    _ => {}
                                }
                            } else if name == "KEY_MACRO_RECORD_START" {
                                // Toggle MR on the release edge. This gives one reliable
                                // transition per physical press and avoids a second press
                                // being lost while the recorder is active.
                                toggle_recording(&shared);
                            }
                            continue;
                        }

                        // The dedicated lighting key is fixed hardware behavior, not programmable.
                        // It is consumed here so it never becomes a recording target or uinput action.
                        if name == "KEY_LIGHTS_TOGGLE" {
                            if down {
                                let _ = leds::toggle_rgb();
                            }
                            continue;
                        }

                        // Recording may target the programmable LCD/menu keys.
                        if down && arm_record_target(&shared, &name) {
                            function_visual_until.insert(
                                name.clone(),
                                Instant::now() + Duration::from_millis(if name == "KEY_KBD_LCD_MENU5" { 320 } else { 220 }),
                            );
                            continue;
                        }

                        let lcd_menu = name.starts_with("KEY_KBD_LCD_MENU");
                        if lcd_menu {
                            if down {
                                function_visual_until.insert(
                                    name.clone(),
                                    Instant::now() + Duration::from_millis(if name == "KEY_KBD_LCD_MENU5" { 320 } else { 220 }),
                                );
                                let mut visible = pressed.clone();
                                visible.insert(name.clone());
                                publish(&shared, &visible, x, y, true, "connected via hid-lg-g15");
                            }

                            if has_mapping(&shared, &name) {
                                set_control(&shared, &keyboard, &mut pressed, &name, down);
                            } else if down {
                                lcd_button(&shared, &name);
                            }
                            continue;
                        }

                        set_control(&shared, &keyboard, &mut pressed, &name, down);
                    } else if device.kind == DeviceKind::Thumbstick && event.type_ == input::EV_ABS {
                        match event.code {
                            input::ABS_X => x = event.value.clamp(0, 255) as u8,
                            input::ABS_Y => y = event.value.clamp(0, 255) as u8,
                            _ => {}
                        }
                        update_directions(&shared, &keyboard, &mut pressed, x, y);
                    }
                }
            }

            let now = Instant::now();
            function_visual_until.retain(|_, until| now < *until);
            let mut visible_pressed = pressed.clone();
            visible_pressed.extend(function_visual_until.keys().cloned());
            publish(&shared, &visible_pressed, x, y, true, "connected via hid-lg-g15");

            // Input-monitor LCD page follows hardware activity without continuously
            // hammering the HID output endpoint when nothing has changed.
            if had_input {
                let monitor_active = {
                    let sh = shared.lock().unwrap();
                    sh.profile.lcd_enabled && sh.profile.lcd_page == 2
                };
                if monitor_active { refresh_lcd_shared(&shared); }
            }

            thread::sleep(Duration::from_millis(5));
        }

        let held: Vec<String> = pressed.iter().cloned().collect();
        for name in held { set_control(&shared, &keyboard, &mut pressed, &name, false); }
        publish(&shared, &HashSet::new(), x, y, false, "G13 disconnected");
        thread::sleep(Duration::from_secs(1));
    }
}
