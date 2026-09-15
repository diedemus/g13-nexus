#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
NAME="g13-nexus"
TOPDIR="${HOME}/rpmbuild"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

sudo dnf install -y \
  rpm-build rpmdevtools cargo rust gcc pkgconf-pkg-config \
  libX11-devel libXcursor-devel libXi-devel libXrandr-devel \
  libxcb-devel libxkbcommon-devel wayland-devel mesa-libGL-devel \
  systemd-devel

mkdir -p "$TOPDIR"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
mkdir -p "$WORK/${NAME}-${VERSION}"

tar --exclude='./target' --exclude='./dist' --exclude='./.git' \
  -C "$ROOT" -cf - . | tar -C "$WORK/${NAME}-${VERSION}" -xf -

tar -C "$WORK" -czf "$TOPDIR/SOURCES/${NAME}-${VERSION}.tar.gz" "${NAME}-${VERSION}"
cp "$ROOT/packaging/g13-nexus.spec" "$TOPDIR/SPECS/g13-nexus.spec"

rpmbuild -ba "$TOPDIR/SPECS/g13-nexus.spec"

mkdir -p "$ROOT/dist"
find "$TOPDIR/RPMS" -type f -name "${NAME}-${VERSION}-*.rpm" -exec cp -v {} "$ROOT/dist/" \;
find "$TOPDIR/SRPMS" -type f -name "${NAME}-${VERSION}-*.src.rpm" -exec cp -v {} "$ROOT/dist/" \;
sha256sum "$ROOT"/dist/${NAME}-${VERSION}-*.rpm > "$ROOT/dist/SHA256SUMS"

echo
echo "RPM artifacts:"
ls -lh "$ROOT/dist/"
