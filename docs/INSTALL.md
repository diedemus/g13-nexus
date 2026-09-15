# Installation and cleanup

## Recommended: RPM

Use the release RPM on Nobara/Fedora systems.

```bash
sudo dnf install ./g13-nexus-1.0.0-1*.rpm
sudo usermod -aG g13-nexus "$USER"
```

Log out and back in once after adding the group, then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now g13-nexus.service
g13-gui
```

## Cleaning legacy development installs

The repository includes `scripts/cleanup-legacy.sh`. It removes the historical hand-installed binaries/services and old G13 Nexus udev rules while deliberately preserving `~/.config/g13-nexus/` and therefore your profiles.

Run it before installing the RPM if this machine has been used for development builds:

```bash
./scripts/cleanup-legacy.sh
```

## Build from source

```bash
sudo dnf install -y \
  cargo rust gcc pkgconf-pkg-config \
  libX11-devel libXcursor-devel libXi-devel libXrandr-devel \
  libxcb-devel libxkbcommon-devel wayland-devel mesa-libGL-devel \
  systemd-devel
cargo build --release
```

The RPM is preferred for a normal installation because it owns the binaries, desktop entry, udev policy, and systemd user unit cleanly.
