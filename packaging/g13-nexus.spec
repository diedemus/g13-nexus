Name:           g13-nexus
Version:        1.0.0
Release:        1%{?dist}
Summary:        Linux configuration and remapping suite for the Logitech G13
License:        MIT
URL:            https://github.com/%{?github_owner}%{!?github_owner:diedemus}/g13-nexus
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  libX11-devel
BuildRequires:  libXcursor-devel
BuildRequires:  libXi-devel
BuildRequires:  libXrandr-devel
BuildRequires:  libxcb-devel
BuildRequires:  libxkbcommon-devel
BuildRequires:  wayland-devel
BuildRequires:  mesa-libGL-devel
BuildRequires:  systemd-devel
Requires(pre):  shadow-utils
Requires:       systemd

%description
G13 Nexus is a native Rust configuration, remapping, macro, RGB lighting, LCD,
and joystick-calibration suite for the Logitech G13 Advanced Gameboard using
the Linux hid-lg-g15 driver.

%prep
%autosetup

%build
cargo build --release

%install
install -Dm0755 target/release/g13-daemon %{buildroot}%{_bindir}/g13-daemon
install -Dm0755 target/release/g13-gui %{buildroot}%{_bindir}/g13-gui
install -Dm0755 target/release/g13ctl %{buildroot}%{_bindir}/g13ctl
install -Dm0644 packaging/g13-nexus.service %{buildroot}%{_userunitdir}/g13-nexus.service
install -Dm0644 packaging/99-g13-nexus.rules %{buildroot}%{_udevrulesdir}/99-g13-nexus.rules
install -Dm0644 packaging/g13-nexus.desktop %{buildroot}%{_datadir}/applications/g13-nexus.desktop

%pre
getent group g13-nexus >/dev/null || groupadd -r g13-nexus

%post
udevadm control --reload-rules >/dev/null 2>&1 || :

%postun
udevadm control --reload-rules >/dev/null 2>&1 || :

%files
%license LICENSE
%doc README.md CHANGELOG.md docs
%{_bindir}/g13-daemon
%{_bindir}/g13-gui
%{_bindir}/g13ctl
%{_userunitdir}/g13-nexus.service
%{_udevrulesdir}/99-g13-nexus.rules
%{_datadir}/applications/g13-nexus.desktop

%changelog
* Tue Sep 15 2026 G13 Nexus contributors <noreply@example.invalid> - 1.0.0-1
- First stable release
