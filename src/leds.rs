use anyhow::{Context,Result}; use std::{fs,path::Path};
const RGB:&str="/sys/class/leds/g13:rgb:kbd_backlight";
pub fn set_rgb(rgb:[u8;3])->Result<()>{let b=Path::new(RGB);fs::write(b.join("multi_intensity"),format!("{} {} {}\n",rgb[0],rgb[1],rgb[2])).context("write G13 multi_intensity")?;let m=fs::read_to_string(b.join("max_brightness")).unwrap_or_else(|_|"255".into());fs::write(b.join("brightness"),m.trim()).context("write G13 brightness")?;Ok(())}
fn led(name:&str,on:bool)->Result<()>{fs::write(format!("/sys/class/leds/{name}/brightness"),if on{"1\n"}else{"0\n"}).with_context(||format!("write {name}"))}
pub fn set_bank(bank:u8)->Result<()>{for i in 1..=3{led(&format!("g13:red:macro_preset_{i}"),i==bank)?;}Ok(())}
pub fn set_record(on:bool)->Result<()>{led("g13:red:macro_record",on)}


pub fn toggle_rgb() -> Result<bool> {
    let b = Path::new(RGB);
    let current = fs::read_to_string(b.join("brightness"))
        .context("read G13 brightness")?
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    if current > 0 {
        fs::write(b.join("brightness"), "0\n").context("disable G13 brightness")?;
        Ok(false)
    } else {
        let max = fs::read_to_string(b.join("max_brightness")).unwrap_or_else(|_| "255".into());
        fs::write(b.join("brightness"), format!("{}\n", max.trim()))
            .context("enable G13 brightness")?;
        Ok(true)
    }
}
