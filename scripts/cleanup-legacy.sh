#!/usr/bin/env bash
set -euo pipefail

echo "Cleaning legacy G13 Nexus development installs."
echo "Profiles in ~/.config/g13-nexus are preserved."

for unit in g13-nexus.service g13-nexus-rs.service g13-daemon.service; do
  systemctl --user disable --now "$unit" 2>/dev/null || true
done

pkill -u "$USER" -x g13-daemon 2>/dev/null || true
pkill -u "$USER" -x g13-gui 2>/dev/null || true

rm -f \
  "$HOME/.config/systemd/user/g13-nexus.service" \
  "$HOME/.config/systemd/user/g13-nexus-rs.service" \
  "$HOME/.config/systemd/user/g13-daemon.service"

sudo rm -f \
  /usr/local/bin/g13-daemon \
  /usr/local/bin/g13-gui \
  /usr/local/bin/g13ctl

# Remove historical unowned /usr/bin copy only if no RPM owns it.
if [[ -e /usr/bin/g13-daemon ]] && ! rpm -qf /usr/bin/g13-daemon >/dev/null 2>&1; then
  sudo rm -f /usr/bin/g13-daemon
fi

sudo rm -f \
  /etc/udev/rules.d/99-g13-nexus.rules \
  /etc/udev/rules.d/99-logitech-g13.rules \
  /etc/udev/rules.d/99-g13.rules

systemctl --user daemon-reload
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input --action=add || true
sudo udevadm trigger --subsystem-match=hidraw --action=add || true
sudo udevadm trigger --subsystem-match=leds --action=add || true

echo
echo "Legacy install cleaned. User profiles were not removed."
echo "After the RPM is installed, enable: systemctl --user enable --now g13-nexus.service"
