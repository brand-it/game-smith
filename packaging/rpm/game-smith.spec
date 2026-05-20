Name:    game-smith
Version: %{version}
Release: 5%{?dist}
Summary: Cross-platform game server management tool
License: MIT
URL:     https://github.com/brand-it/game-smith
BuildArch: x86_64
Requires: gtk3 libappindicator-gtk3

%description
Cross-platform game server management tool with system tray integration.

%install
install -Dm755 %{srcdir}/target/release/game-smith \
               %{buildroot}/usr/bin/game-smith
install -Dm644 %{srcdir}/assets/app/game-smith.desktop \
               %{buildroot}/usr/share/applications/game-smith.desktop
install -Dm644 %{srcdir}/assets/icons/game-smith-48.png \
               %{buildroot}/usr/share/icons/hicolor/48x48/apps/game-smith.png
install -Dm644 %{srcdir}/assets/icons/game-smith-256.png \
               %{buildroot}/usr/share/icons/hicolor/256x256/apps/game-smith.png
install -Dm644 %{srcdir}/config/production.yaml \
               %{buildroot}/usr/share/game-smith/config/production.yaml
mkdir -p %{buildroot}/usr/share/game-smith/assets
cp -r %{srcdir}/assets/views  %{buildroot}/usr/share/game-smith/assets/views
cp -r %{srcdir}/assets/i18n   %{buildroot}/usr/share/game-smith/assets/i18n


%files
%{_bindir}/game-smith
%{_datadir}/applications/game-smith.desktop
%{_datadir}/icons/hicolor/48x48/apps/game-smith.png
%{_datadir}/icons/hicolor/256x256/apps/game-smith.png
%{_datadir}/game-smith/assets/views/
%{_datadir}/game-smith/assets/i18n/
%{_datadir}/game-smith/config/production.yaml

%changelog
* Sun May 17 2026 brand-it <brandit@example.com> - %{version}-1
- Initial RPM package
