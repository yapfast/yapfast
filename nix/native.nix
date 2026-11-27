# The native parts of a Linux build, shared by the dev shell and the package:
# LiveKit's libwebrtc and the system libraries.
{ pkgs }:
let
  arch = if pkgs.stdenv.hostPlatform.isAarch64 then "arm64" else "x64";
in
rec {
  webrtcArchive = pkgs.fetchurl {
    url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-51ef663/webrtc-linux-${arch}-release.zip";
    hash = {
      x64 = "sha256-3OnZQUzY4syaRxwFRIGFGyV60qh5ESzlvlY2m0/v2u0=";
      arm64 = "sha256-tVymLCixjcW7cgpwgq5GjXjyE8o5vMz4QZXy/ljP5xM=";
    }.${arch};
  };
  webrtc = pkgs.runCommand "talk-libwebrtc-51ef663" {
    nativeBuildInputs = [ pkgs.unzip ];
  } ''
    unzip -q ${webrtcArchive} -d unpacked
    mv unpacked/linux-${arch}-release "$out"
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
