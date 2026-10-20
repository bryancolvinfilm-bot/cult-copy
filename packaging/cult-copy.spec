%global debug_package %{nil}
%global _build_id_links none

# Replace the earlier package while exposing the canonical Cult Copy name.
Name:           cult-copy
Version:        %{app_version}
Release:        1%{?dist}
Summary:        Private clipboard sharing between paired computers
License:        MIT
URL:            https://github.com/bryancolvinfilm-bot/cult-copy
Source0:        %{name}-%{version}.tar.gz
Provides:       duet-clipboard = %{version}-%{release}
Obsoletes:      duet-clipboard < %{version}-%{release}
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
Requires:       libxkbcommon-x11.so.0()(64bit)
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libEGL.so.1()(64bit)
Requires:       libGL.so.1()(64bit)

%description
An independent, open-source tray app for sharing text, images, files, and
folders with paired computers on a local network. No cloud service or account
is required.

%prep
%setup -q

%install
install -Dpm 755 cult-copy %{buildroot}%{_bindir}/cult-copy
desktop-file-install --dir=%{buildroot}%{_datadir}/applications cult-copy.desktop
install -Dpm 644 cult-copy.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/cult-copy.svg
install -Dpm 644 cult-copy.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/cult-copy.png
install -Dpm 644 io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml %{buildroot}%{_datadir}/metainfo/io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/cult-copy.desktop

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
%{_bindir}/cult-copy
%{_datadir}/applications/cult-copy.desktop
%{_datadir}/icons/hicolor/scalable/apps/cult-copy.svg
%{_datadir}/icons/hicolor/256x256/apps/cult-copy.png
%{_datadir}/metainfo/io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml
