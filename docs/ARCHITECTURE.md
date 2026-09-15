# Architecture

G13 Nexus deliberately uses the kernel's native Logitech G13 support as its hardware input layer.

## Input path

`hid-lg-g15` exposes the G13 as evdev devices. `g13-daemon` discovers devices by their kernel identity and Logitech USB VID/PID `046d:c21c`, then reads input events directly. The G13 keypad and thumbstick are exclusively grabbed so desktop software does not also receive the raw macro/special-key events.

The daemon converts configured bindings and macros into a virtual keyboard through Linux uinput.

## GUI and daemon IPC

The GUI and `g13ctl` communicate with the daemon through a Unix socket at `$XDG_RUNTIME_DIR/g13-nexus.sock`.

Frequently polled status is maintained in a separate cache so slow profile, LCD, or LED work cannot stall the GUI feedback path. This is important for the M keys and MR state, which need to feel as immediate as the G keys during gaming.

## Lighting

Linux exposes the G13 lighting endpoints through the LED class. G13 Nexus controls:

- `g13:rgb:kbd_backlight`
- `g13:red:macro_preset_1`
- `g13:red:macro_preset_2`
- `g13:red:macro_preset_3`
- `g13:red:macro_record`

The RPM creates a dedicated `g13-nexus` group and narrow udev rules that make only these G13 LED attributes group-writable.

## LCD

The G13 LCD is 160x43 monochrome. The framebuffer is packed into six horizontal 8-pixel bands and transmitted through the G13 hidraw output report. The LCD path is output-only; normal input remains evdev-based.

## Joystick

The daemon reads the current ABS_X and ABS_Y values on connect to establish the hardware startup center. Profiles can optionally override that center. Directional mappings use independent X/Y thresholds plus hysteresis to avoid chatter near the dead-zone boundary.

## Security model

- no root daemon
- no `input` group membership
- no global hidraw permissions
- exact G13 evdev/hidraw nodes use logind `uaccess`
- only exact G13 LED attributes use the dedicated `g13-nexus` group
