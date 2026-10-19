%global debug_package %{nil}
%global _build_id_links none

Name:           duet-clipboard
Version:        %{app_version}
Release:        1%{?dist}
Summary:        Private clipboard sharing between paired computers
License:        MIT
URL:            https://github.com/bryancolvinfilm-bot/duet-clipboard
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  desktop-file-utils
Requires:       hicolor-icon-theme
# The window and clipboard backends load these libraries at runtime, so RPM's
# ELF dependency generator cannot discover every desktop dependency itself.
Requires:       libX11.so.6()(64bit)
Requires:       libXcursor.so.1()(64bit)
Requires:       libXi.so.6()(64bit)
Requires:       libXrandr.so.2()(64bit)
Requires:       libXinerama.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libEGL.so.1()(64bit)
Requires:       libGL.so.1()(64bit)

%description
An independent, open-source tray app for sharing text, images, files, and
folders with paired computers on a local network. No cloud service or account
is required. This project is not affiliated with Duet Display.

%prep
%setup -q

%install
install -Dpm 755 duet-clipboard %{buildroot}%{_bindir}/duet-clipboard
desktop-file-install --dir=%{buildroot}%{_datadir}/applications duet-clipboard.desktop
install -Dpm 644 duet-clipboard.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/duet-clipboard.svg
install -Dpm 644 duet-clipboard.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/duet-clipboard.png

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/duet-clipboard.desktop

%post
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor >/dev/null 2>&1 || :
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database %{_datadir}/applications >/dev/null 2>&1 || :
fi

%postun
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor >/dev/null 2>&1 || :
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database %{_datadir}/applications >/dev/null 2>&1 || :
fi

%files
%license LICENSE
%doc NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
%{_bindir}/duet-clipboard
%{_datadir}/applications/duet-clipboard.desktop
%{_datadir}/icons/hicolor/scalable/apps/duet-clipboard.svg
%{_datadir}/icons/hicolor/256x256/apps/duet-clipboard.png
