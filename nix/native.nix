# The native parts of a Linux build, shared by the dev shell and the package:
# LiveKit's libwebrtc and the system libraries.
{ pkgs }:
rec {
  webrtcArchive = pkgs.fetchurl {
    url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-a545de4/webrtc-linux-x64-release.zip";
    hash = "sha256-Ls5zuacqLKzXPQxirDpBTiAvN4hcPI9J3dtDwUQ6wL0=";
  };
  webrtc = pkgs.runCommand "talk-libwebrtc-a545de4" {
    nativeBuildInputs = [ pkgs.unzip ];
  } ''
    unzip -q ${webrtcArchive} -d unpacked
    mv unpacked/linux-x64-release "$out"
  '';
  libraries = with pkgs; [
    alsa-lib libpulseaudio fontconfig freetype glib libxkbcommon openssl
    wayland libGL
    xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
    xorg.libxcb
    # libwebrtc's screen capture loads these at runtime: PipeWire and GBM
    # for the screen sharing portal, the X extensions on X11.
    pipewire libdrm libgbm
    xorg.libXfixes xorg.libXdamage xorg.libXcomposite xorg.libXext
  ];
  # What the binary loads at runtime, on the wrapper's library path: the
  # libraries, and libva, which FFmpeg's VA-API encoders load when a stream
  # starts on an AMD or Intel GPU, and which finds the drivers in
  # /run/opengl-driver/lib/dri. Not a build input: webrtc-sys would build
  # LiveKit's own VA-API encoder where pkg-config finds libva. The Vulkan
  # loader is what the GPU renderer (wgpu) opens to reach the
  # drivers' Vulkan; NixOS's /run/opengl-driver/lib has the drivers but not
  # the loader.
  runtimeLibraries = libraries ++ [ pkgs.libva pkgs.vulkan-loader ];
}
