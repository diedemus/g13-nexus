#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

sudo dnf install -y \
  cargo rust gcc pkgconf-pkg-config \
  libX11-devel libXcursor-devel libXi-devel libXrandr-devel \
  libxcb-devel libxkbcommon-devel wayland-devel mesa-libGL-devel \
  systemd-devel

cargo build --release

systemctl --user disable --now g13-nexus.service 2>/dev/null || true
sudo install -Dm0755 target/release/g13-daemon /usr/local/bin/g13-daemon
sudo install -Dm0755 target/release/g13-gui /usr/local/bin/g13-gui
sudo install -Dm0755 target/release/g13ctl /usr/local/bin/g13ctl

mkdir -p "$HOME/.config/systemd/user"
sed 's#/usr/bin/g13-daemon#/usr/local/bin/g13-daemon#' packaging/g13-nexus.service \
  > "$HOME/.config/systemd/user/g13-nexus.service"

sudo getent group g13-nexus >/dev/null || sudo groupadd -r g13-nexus
if ! id -nG "$USER" | tr ' ' '\n' | grep -qx g13-nexus; then
  echo "Add your user to the LED-access group, then log out/in once:"
  echo "  sudo usermod -aG g13-nexus \"$USER\""
fi

sudo install -Dm0644 packaging/99-g13-nexus.rules /etc/udev/rules.d/99-g13-nexus.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input --action=add || true
sudo udevadm trigger --subsystem-match=hidraw --action=add || true
sudo udevadm trigger --subsystem-match=leds --action=add || true

systemctl --user daemon-reload
systemctl --user enable --now g13-nexus.service
sleep 1
g13ctl status || true
