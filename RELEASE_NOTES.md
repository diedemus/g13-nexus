# G13 Nexus 1.0.0

First stable release of the kernel-backed Logitech G13 Linux suite.

## Included

- Low-latency G1-G22 remapping with M1/M2/M3 banks.
- MR macro recording, live recording monitor, timing capture, playback, and macro indicators.
- RGB lighting plus M/MR LEDs.
- Functional 160x43 G13 LCD with Status, Custom Text, Input Monitor, and Image pages.
- Programmable LCD-area controls with built-in behavior when unbound.
- Thumb buttons, stick click, four joystick directions, dead zone/hysteresis, and center calibration.
- Named profiles with per-bank mappings/macros and per-profile LCD/lighting/joystick settings.
- Nobara/Fedora RPM packaging, systemd user service, and narrow G13-only permissions.

## Upgrade from development builds

If you previously installed development binaries into `/usr/local/bin` or user service files manually, run:

```bash
./scripts/cleanup-legacy.sh
```

This preserves `~/.config/g13-nexus/` profiles.

Then install the RPM and ensure your user belongs to `g13-nexus` for LED sysfs access.
