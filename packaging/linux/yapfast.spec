Name:           yapfast
Version:        %{yapfast_version}
Release:        1
Summary:        Native Discord client
License:        GPL-3.0-or-later
URL:            https://github.com/yapfast/yapfast

# ELF dependencies are generated automatically. These libraries are opened at runtime.
Requires:       hicolor-icon-theme, alsa-lib, libwayland-client, libxkbcommon
Requires:       libX11, libxcb, libXcursor, libXi, libxkbcommon-x11
Recommends:     pipewire-libs, libdrm, mesa-libgbm, vulkan-loader

# The prebuilt binary was stripped by Nix.
%global debug_package %{nil}
%global __os_install_post %{nil}

%description
Yapfast is a native desktop client for Discord text, voice and video.

%install
mkdir -p "%{buildroot}"
cp -a "%{yapfast_payload}/usr" "%{buildroot}/"
install -Dm644 "%{buildroot}%{_docdir}/yapfast/LICENSE" "%{buildroot}%{_licensedir}/yapfast/LICENSE"
rm "%{buildroot}%{_docdir}/yapfast/LICENSE"

%files
%{_bindir}/yapfast
%{_datadir}/applications/io.github.yapfast.Yapfast.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.yapfast.Yapfast.svg
%{_datadir}/metainfo/io.github.yapfast.Yapfast.metainfo.xml
%doc %{_docdir}/yapfast
%license %{_licensedir}/yapfast
