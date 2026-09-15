# G13 Nexus 1.0.0

G13 Nexus is a native Rust configuration, remapping, macro, lighting, and LCD suite for the Logitech G13 Advanced Gameboard on modern Linux systems using the kernel `hid-lg-g15` driver.

The project is built around the Linux kernel's native G13 support rather than replacing it with a custom USB input driver. Physical input is consumed from evdev, mapped output is emitted through uinput, LEDs are controlled through the Linux LED class, and the 160x43 LCD is driven through the G13 hidraw output interface.

## Highlights

- G1-G22 remapping with three M banks.
- M1/M2/M3 hardware bank selection and LEDs.
- MR macro recording with live recording window, key timing, stored macros, and macro-bound indicators.
- Programmable LCD buttons while retaining their built-in page controls when unbound.
- LCD Status, Custom Text, Input Monitor, and Image pages.
- PNG/JPEG/BMP LCD image support with scale, zoom, and positioning controls.
- RGB backlight control and dedicated hardware lighting toggle.
- Thumb buttons, stick click, and four programmable joystick directions.
- Adjustable dead zone, hysteresis, live X/Y monitor, and per-profile joystick center calibration.
- Named profiles with independent M-bank bindings, macros, lighting, LCD configuration, and joystick settings.
- Exclusive G13 evdev capture to prevent raw macro/special-key leakage to the desktop.
- Hardware discovery by Logitech VID/PID `046d:c21c`; no hard-coded `/dev/input/eventN` or `/dev/hidrawN` paths.
- Low-latency cached GUI status path designed for gaming use.
- systemd user service; the daemon does not run as root.

## Requirements

G13 Nexus targets Nobara/Fedora-class Linux systems with a kernel new enough to provide `hid-lg-g15` support for the G13. The tested development environment is Nobara 44 GNOME.

The device should appear as kernel input devices named `Logitech G13 Gaming Keypad` and `Logitech G13 Thumbstick` and expose the corresponding LED-class entries under `/sys/class/leds/`.

## Install from RPM

Install the release RPM, then ensure your user is in the dedicated `g13-nexus` group used only for the G13 LED sysfs attributes:

```bash
sudo dnf install ./g13-nexus-1.0.0-1*.rpm
sudo usermod -aG g13-nexus "$USER"
```

Log out and back in after the first group addition. Then enable the user service:

```bash
systemctl --user daemon-reload
systemctl --user enable --now g13-nexus.service
g13-gui
```

The package uses `uaccess` for the exact G13 evdev/hidraw devices. It does not add you to the global `input` group and does not make hidraw devices world-writable.

## Build from source

```bash
sudo dnf install -y \
  cargo rust gcc pkgconf-pkg-config \
  libX11-devel libXcursor-devel libXi-devel libXrandr-devel \
  libxcb-devel libxkbcommon-devel wayland-devel mesa-libGL-devel \
  systemd-devel

cargo build --release
```

For a local developer install, see [`docs/INSTALL.md`](docs/INSTALL.md).

## RPM build

```bash
./scripts/build-rpm.sh
```

The resulting RPMs are copied to `dist/`.

## Runtime commands

```bash
g13-gui
g13ctl status
g13ctl reload
systemctl --user status g13-nexus.service
journalctl --user -u g13-nexus.service -f
```

## Configuration

Profiles and application state live under:

```text
~/.config/g13-nexus/
```

Removing or reinstalling the RPM does not delete your profiles.

## Architecture

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the input, output, IPC, LCD, LED, and permissions design.

## Macro recorder

See [`docs/MACROS.md`](docs/MACROS.md).

## Hardware behavior and controls

See [`docs/HARDWARE.md`](docs/HARDWARE.md).

## Known scope

On-device profile-memory / Logitech "Profiles To Go" support is intentionally deferred. G13 Nexus currently stores profiles on the host. Onboard-memory writes will only be implemented after the device protocol is verified and can be read back safely.

## License

MIT. See [`LICENSE`](LICENSE).
