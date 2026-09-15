use anyhow::{Context, Result};
use crate::config::LcdAlign;
use image::{imageops::FilterType, GenericImageView};
use std::{fs, fs::OpenOptions, io::Write, path::PathBuf};

pub const W: usize = 160;
pub const H: usize = 43;
const BANDS: usize = 6;
const BUFFER: usize = W * BANDS;
const REPORT_SIZE: usize = 992;
const IMAGE_OFFSET: usize = 32;

pub struct Frame {
    pub buffer: [u8; BUFFER],
}

impl Frame {
    pub fn new() -> Self { Self { buffer: [0; BUFFER] } }

    pub fn set(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 || x as usize >= W || y as usize >= H {
            return;
        }
        // G13 LCD layout: six horizontal 8-pixel bands. Each byte is one
        // X column inside the band, and its bits are the Y pixels.
        let index = (y as usize / 8) * W + x as usize;
        let mask = 1u8 << (y as usize % 8);
        if on {
            self.buffer[index] |= mask;
        } else {
            self.buffer[index] &= !mask;
        }
    }

    pub fn hline(&mut self, y: i32) {
        for x in 0..W as i32 { self.set(x, y, true); }
    }

    pub fn text(&mut self, mut x: i32, y: i32, text: &str, scale: i32) {
        for c in text.chars() {
            self.character(x, y, c, scale.max(1));
            x += 6 * scale.max(1);
            if x >= W as i32 - 4 { break; }
        }
    }

    pub fn text_aligned(&mut self, y: i32, text: &str, scale: i32, align: LcdAlign) {
        let scale = scale.max(1);
        let width = (text.chars().count() as i32 * 6 * scale).saturating_sub(scale);
        let x = match align {
            LcdAlign::Left => 2,
            LcdAlign::Center => ((W as i32 - width) / 2).max(0),
            LcdAlign::Right => (W as i32 - 2 - width).max(0),
        };
        self.text(x, y, text, scale);
    }

    pub fn image_path_adjusted(
        &mut self,
        path: &str,
        scale_x: f32,
        scale_y: f32,
        zoom: f32,
        anchor_x: f32,
        anchor_y: f32,
    ) -> Result<()> {
        let img = image::open(path).with_context(|| format!("open LCD image {path}"))?;
        let (src_w, src_h) = img.dimensions();
        if src_w == 0 || src_h == 0 {
            anyhow::bail!("LCD image has zero dimensions");
        }

        // Start from aspect-preserving fit-to-screen, then apply independent X/Y
        // scaling and an overall zoom. Dimensions may exceed the LCD; the image
        // is centered and naturally cropped by Frame::set at the display edges.
        let fit = f32::min(W as f32 / src_w as f32, H as f32 / src_h as f32);
        let sx = scale_x.clamp(0.25, 3.0);
        let sy = scale_y.clamp(0.25, 3.0);
        let zoom = zoom.clamp(0.25, 4.0);
        let dst_w = ((src_w as f32 * fit * sx * zoom).round() as u32)
            .clamp(1, (W * 8) as u32);
        let dst_h = ((src_h as f32 * fit * sy * zoom).round() as u32)
            .clamp(1, (H * 8) as u32);

        let gray = img.to_luma8();
        let resized = image::imageops::resize(&gray, dst_w, dst_h, FilterType::Triangle);
        let centered_x = (W as i32 - dst_w as i32) / 2;
        let centered_y = (H as i32 - dst_h as i32) / 2;
        let travel_x = ((W as i32 + dst_w as i32) / 2).max(1) as f32;
        let travel_y = ((H as i32 + dst_h as i32) / 2).max(1) as f32;
        let ox = centered_x + (anchor_x.clamp(-1.0, 1.0) * travel_x).round() as i32;
        let oy = centered_y + (anchor_y.clamp(-1.0, 1.0) * travel_y).round() as i32;
        for yy in 0..dst_h {
            for xx in 0..dst_w {
                let on = resized.get_pixel(xx, yy).0[0] < 128;
                self.set(ox + xx as i32, oy + yy as i32, on);
            }
        }
        Ok(())
    }

    pub fn pixel(&self, x: usize, y: usize) -> bool {
        if x >= W || y >= H {
            return false;
        }
        let index = (y / 8) * W + x;
        self.buffer[index] & (1u8 << (y % 8)) != 0
    }

    fn character(&mut self, x: i32, y: i32, c: char, scale: i32) {
        let glyph = glyph(c);
        for (col, byte) in glyph.iter().enumerate() {
            for row in 0..7 {
                if byte & (1 << row) != 0 {
                    for dx in 0..scale {
                        for dy in 0..scale {
                            self.set(x + col as i32 * scale + dx, y + row * scale + dy, true);
                        }
                    }
                }
            }
        }
    }

    pub fn report(&self) -> [u8; REPORT_SIZE] {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = 0x03;
        report[IMAGE_OFFSET..IMAGE_OFFSET + BUFFER].copy_from_slice(&self.buffer);
        report
    }
}

fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        'A'=>[0x7e,0x11,0x11,0x11,0x7e],'B'=>[0x7f,0x49,0x49,0x49,0x36],
        'C'=>[0x3e,0x41,0x41,0x41,0x22],'D'=>[0x7f,0x41,0x41,0x22,0x1c],
        'E'=>[0x7f,0x49,0x49,0x49,0x41],'F'=>[0x7f,0x09,0x09,0x09,0x01],
        'G'=>[0x3e,0x41,0x49,0x49,0x7a],'H'=>[0x7f,0x08,0x08,0x08,0x7f],
        'I'=>[0x00,0x41,0x7f,0x41,0x00],'J'=>[0x20,0x40,0x41,0x3f,0x01],
        'K'=>[0x7f,0x08,0x14,0x22,0x41],'L'=>[0x7f,0x40,0x40,0x40,0x40],
        'M'=>[0x7f,0x02,0x0c,0x02,0x7f],'N'=>[0x7f,0x04,0x08,0x10,0x7f],
        'O'=>[0x3e,0x41,0x41,0x41,0x3e],'P'=>[0x7f,0x09,0x09,0x09,0x06],
        'Q'=>[0x3e,0x41,0x51,0x21,0x5e],'R'=>[0x7f,0x09,0x19,0x29,0x46],
        'S'=>[0x46,0x49,0x49,0x49,0x31],'T'=>[0x01,0x01,0x7f,0x01,0x01],
        'U'=>[0x3f,0x40,0x40,0x40,0x3f],'V'=>[0x1f,0x20,0x40,0x20,0x1f],
        'W'=>[0x3f,0x40,0x38,0x40,0x3f],'X'=>[0x63,0x14,0x08,0x14,0x63],
        'Y'=>[0x07,0x08,0x70,0x08,0x07],'Z'=>[0x61,0x51,0x49,0x45,0x43],
        '0'=>[0x3e,0x51,0x49,0x45,0x3e],'1'=>[0x00,0x42,0x7f,0x40,0x00],
        '2'=>[0x42,0x61,0x51,0x49,0x46],'3'=>[0x21,0x41,0x45,0x4b,0x31],
        '4'=>[0x18,0x14,0x12,0x7f,0x10],'5'=>[0x27,0x45,0x45,0x45,0x39],
        '6'=>[0x3c,0x4a,0x49,0x49,0x30],'7'=>[0x01,0x71,0x09,0x05,0x03],
        '8'=>[0x36,0x49,0x49,0x49,0x36],'9'=>[0x06,0x49,0x49,0x29,0x1e],
        '-'=>[0x08,0x08,0x08,0x08,0x08],':'=>[0,0x36,0x36,0,0],
        '.'=>[0,0x60,0x60,0,0],'/'=>[0x20,0x10,0x08,0x04,0x02],
        '+'=>[0x08,0x08,0x3e,0x08,0x08],' '=>[0;5],
        _=>[0x02,0x01,0x51,0x09,0x06],
    }
}

pub fn image_preview(
    path: &str,
    scale_x: f32,
    scale_y: f32,
    zoom: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> Result<Vec<bool>> {
    let mut frame = Frame::new();
    frame.image_path_adjusted(path, scale_x, scale_y, zoom, anchor_x, anchor_y)?;
    let mut out = Vec::with_capacity(W * H);
    for y in 0..H {
        for x in 0..W {
            out.push(frame.pixel(x, y));
        }
    }
    Ok(out)
}

pub fn discover() -> Result<PathBuf> {
    for entry in fs::read_dir("/sys/class/hidraw").context("read hidraw sysfs")? {
        let entry = entry?;
        let device = fs::canonicalize(entry.path().join("device")).unwrap_or_default();
        if device.to_string_lossy().contains("046D:C21C") {
            return Ok(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    anyhow::bail!("G13 hidraw not found")
}

pub fn write_frame(frame: &Frame) -> Result<()> {
    let path = discover()?;
    let mut hidraw = OpenOptions::new()
        .write(true)
        .open(&path)
        .with_context(|| format!("open {} for LCD output", path.display()))?;
    hidraw
        .write_all(&frame.report())
        .context("write G13 LCD output report")
}

pub fn clear() -> Result<()> { write_frame(&Frame::new()) }

pub fn show(
    profile: &str,
    bank: u8,
    recording: bool,
    enabled: bool,
    page: u8,
    custom: &[String; 4],
    align: &[LcdAlign; 4],
    image_path: &str,
    image_scale_x: f32,
    image_scale_y: f32,
    image_zoom: f32,
    image_anchor_x: f32,
    image_anchor_y: f32,
    x: u8,
    y: u8,
    pressed: &[String],
) -> Result<()> {
    if !enabled { return clear(); }

    let mut frame = Frame::new();
    match page % 4 {
        0 => {
            frame.text(2, 1, "G13 NEXUS", 1);
            frame.hline(9);
            frame.text(2, 12, &format!("PROFILE: {profile}"), 1);
            frame.text(2, 22, &format!("BANK: M{bank}"), 1);
            frame.text(2, 32, if recording { "MR: RECORDING" } else { "MR: READY" }, 1);
        }
        1 => {
            for (row, line) in custom.iter().enumerate() {
                frame.text_aligned(1 + row as i32 * 10, line, 1, align[row]);
            }
        }
        2 => {
            frame.text(2, 1, "INPUT MONITOR", 1);
            frame.hline(9);
            frame.text(2, 12, &format!("X:{x:03} Y:{y:03}"), 1);
            let p = if pressed.is_empty() {
                "NONE".to_owned()
            } else {
                pressed.join("+")
            };
            frame.text(2, 22, &p, 1);
            frame.text(2, 32, &format!("M{bank} {profile}"), 1);
        }
        _ => {
            if image_path.trim().is_empty() {
                frame.text_aligned(17, "NO IMAGE", 1, LcdAlign::Center);
            } else if frame
                .image_path_adjusted(image_path, image_scale_x, image_scale_y, image_zoom, image_anchor_x, image_anchor_y)
                .is_err()
            {
                frame.text_aligned(12, "IMAGE ERROR", 1, LcdAlign::Center);
                frame.text_aligned(24, "CHECK PATH", 1, LcdAlign::Center);
            }
        }
    }
    write_frame(&frame)
}
