# Changelog

## 1.0.0 - 2026-09-15

First stable G13 Nexus release.

### Input and mappings
- Kernel-backed G13 input through `hid-lg-g15` evdev.
- G1-G22, M1/M2/M3/MR, LCD/menu controls, thumb buttons, stick click, and directional joystick mappings.
- Exclusive device grabs and uinput remapped output.
- Low-latency status path for physical-key feedback.

### Profiles and macros
- Named profiles with three independent M banks.
- MR-driven macro recording with event timing and asynchronous playback.
- Live macro-recording monitor and macro-bound indicators.

### LCD and lighting
- 160x43 monochrome G13 LCD output.
- Status, Custom Text, Input Monitor, and Image pages.
- RGB keyboard backlight and M/MR LEDs.
- Dedicated physical lighting button remains a fixed G13 lighting toggle.

### Joystick
- Dynamic startup center detection.
- Adjustable dead zone and hysteresis.
- Live 2D dead-zone view.
- Per-profile Center X / Center Y overrides with Capture and Reset controls.

### Packaging and security
- Nobara/Fedora RPM packaging.
- systemd user service.
- `uaccess` for the exact G13 input/hidraw nodes.
- Dedicated `g13-nexus` group for only the G13 LED sysfs controls.
- No root daemon, no global `input` group requirement, and no world-writable device nodes.
