# Security

G13 Nexus is designed to avoid broad input-device permissions.

- The daemon runs as the logged-in user, not root.
- The package does not require membership in the global `input` group.
- G13 evdev and hidraw nodes use `TAG+="uaccess"` for the exact USB VID/PID.
- G13 LED sysfs attributes are group-writable only by the dedicated `g13-nexus` group.
- No device node is intentionally made world-writable.

Report security issues privately to the repository owner rather than opening a public issue with sensitive details.
