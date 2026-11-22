# The native parts of a Linux build, shared by the dev shell and the package:
# LiveKit's libwebrtc, the prepared Skia archive, and the system libraries.
{ pkgs }:
rec {
  webrtcArchive = pkgs.fetchurl {
    url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-51ef663/webrtc-linux-x64-release.zip";
    hash = "sha256-3OnZQUzY4syaRxwFRIGFGyV60qh5ESzlvlY2m0/v2u0=";
  };
  webrtc = pkgs.runCommand "talk-libwebrtc-51ef663" {
    nativeBuildInputs = [ pkgs.unzip ];
  } ''
    unzip -q ${webrtcArchive} -d unpacked
    mv unpacked/linux-x64-release "$out"
  '';
  skiaPackage = "skia-binaries-30669978c3f39a73e8a7-x86_64-unknown-linux-gnu-ftembed-ganesh-gl-jpegd-jpege-pdf-textlayout.tar.gz";
  skiaArchive = pkgs.fetchurl {
    url = "https://github.com/rust-skia/skia-binaries/releases/download/0.157.0/${skiaPackage}";
    sha256 = "d5ea4666b67f4ff74a2c9dccf9cbb069fc991552caa9ed26fd4379572ddfe3b1";
  };
  skia = pkgs.runCommand "talk-skia-0.157.0-linux" {
    nativeBuildInputs = [ pkgs.python3 pkgs.binutils pkgs.llvmPackages_21.llvm ];
    YAPFAST_SKIA_SOURCE = skiaArchive;
    YAPFAST_SKIA_OBJCOPY = "${pkgs.llvmPackages_21.llvm}/bin/llvm-objcopy";
    YAPFAST_SKIA_NM = "${pkgs.llvmPackages_21.llvm}/bin/llvm-nm";
  } ''
    export YAPFAST_SKIA_OUTPUT="$out"
    python3 ${./prepare-skia-native.py}
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
  # LiveKit's own VA-API encoder where pkg-config finds libva.
  runtimeLibraries = libraries ++ [ pkgs.libva ];
}
