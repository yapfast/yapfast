# The native parts of a Linux build, shared by the dev shell and the package:
# LiveKit's libwebrtc, the prepared Skia archives — the prebuilt OpenGL one and
# the Vulkan one built from Skia's source — and the system libraries.
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

  # Skia's own source tree with the third-party code its builds read: the two
  # builds that can't use the prebuilt archive above, which is Linux's OpenGL
  # one. `skiaVulkan` below builds Linux's with Vulkan out of it, and the
  # cross shell hands `windowsSkiaSource` to rust-skia for the Windows build.
  skiaSourceArchive = pkgs.fetchurl {
    url = "https://codeload.github.com/rust-skia/skia/tar.gz/6a8fe80eadfac737c12e64229a1b9530bc78f3d1";
    sha256 = "dad0161d334409990f6f4409d1a26c1b0389e1668b79c0e175aecef651fd346d";
  };
  skiaDependency = name: url: sha256: {
    inherit name;
    source = pkgs.fetchzip { inherit url sha256; stripRoot = false; };
  };
  windowsSkiaDependencies = [
    # Skia's mirror of it answers 403 now; GitHub's archive is the same
    # tree under a top folder.
    {
      name = "d3d12allocator";
      source = pkgs.fetchzip {
        url = "https://github.com/GPUOpen-LibrariesAndSDKs/D3D12MemoryAllocator/archive/169895d529dfce00390a20e69c2f516066fe7a3b.tar.gz";
        sha256 = "sha256-5DbR/3Lb0eMBpHNf7MtZOb1C0QrNlx/ZvPVbCxxF9eQ=";
      };
    }
    (skiaDependency "expat" "https://chromium.googlesource.com/external/github.com/libexpat/libexpat/+archive/a70841d9ffa64ddde29ab824ed6305035ebe4c47.tar.gz" "sha256-aRRlOkHq/qYa+7hrJM6LugNkiUz5ki/Sg3mONa5NEzs=")
    (skiaDependency "libjpeg-turbo" "https://chromium.googlesource.com/chromium/deps/libjpeg_turbo/+archive/640f254ad0fa03f6b1f29f89b7dd9366f2f6e533.tar.gz" "sha256-wor4RTF3/5BFL9EWcGEofY+M4HN2+/KJUaOY+u86K5Q=")
    (skiaDependency "libpng" "https://skia.googlesource.com/third_party/libpng/+archive/3061454d980de7d53608f594194cfac722721d2a.tar.gz" "sha256-JSdzPsfRBjnk8DB/fkOCkaiUJ9/neQ2myIK5xnQuSsM=")
    (skiaDependency "wuffs" "https://skia.googlesource.com/external/github.com/google/wuffs-mirror-release-c/+archive/7411f488fe2e2c205c3d3b3d28638b7356522930.tar.gz" "sha256-AMuAaCbNJYnSeac1B1IRSUh4rlE2KphT04/Ak4Pig5M=")
    (skiaDependency "zlib" "https://chromium.googlesource.com/chromium/src/third_party/zlib/+archive/456ae730017b9b4bd3371927abc82627fd51feef.tar.gz" "sha256-K07NY9DN6lsTNmKl7OK3xAZ1vdesSnQVVsN3ZdJG/WU=")
    (skiaDependency "spirv-cross" "https://chromium.googlesource.com/external/github.com/KhronosGroup/SPIRV-Cross/+archive/b8fcf307f1f347089e3c46eb4451d27f32ebc8d3.tar.gz" "sha256-H43M9DXfEuyKuvo6rjb5k0KEbYOSFodbPJh8ZKY4PQg=")
  ];
  # The Linux build also has Skia's own FreeType in it (see `libraries` below);
  # the Windows build draws text through DirectWrite.
  linuxSkiaDependencies = [
    (skiaDependency "freetype" "https://chromium.googlesource.com/chromium/src/third_party/freetype2/+archive/37515cf56ba3349656049edbed89ffc6f6466f2d.tar.gz" "sha256-Mcmb67mYYs6c8yQv39iIq5gxUGBZmWQXkMGSOw6jt14=")
    # Skia's Vulkan backend, which a GL-only prebuilt has no use for; see
    # `skiaVulkan` below and the `vulkan` gn arg in nix/skia-linux-builder.
    (skiaDependency "vulkanmemoryallocator" "https://chromium.googlesource.com/external/github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator/+archive/eb744ea7a2b17040121b4bbb4d6f9e8a77e3cae7.tar.gz" "sha256-LBZJcom7G7maF9wpUVeVEJQAJwGy6365INk3VD0/0PM=")
    # Layout and ICU, which Skia builds its own copies of: the `textlayout`
    # feature turns both on, and Skia's bundled harfbuzz.
    (skiaDependency "harfbuzz" "https://chromium.googlesource.com/external/github.com/harfbuzz/harfbuzz/+archive/12ee77ee00514c442bcd28f9aa9d56b708c37382.tar.gz" "sha256-TPWKUApcxKnbFZmx0XjZeXEqVYWKWruF/MPYydKjKDQ=")
    (skiaDependency "icu" "https://chromium.googlesource.com/chromium/deps/icu/+archive/2a51536dfb3b448bdf3dae70c782ea2879c90904.tar.gz" "sha256-jMH5pspG5VdIjZDBtE4cVgsta4vWKIdzS3O0iTnPTyU=")
  ];
  windowsSkiaSource = pkgs.runCommand "talk-skia-0.157.0-windows-source" { } ''
    mkdir -p "$out"
    tar -xzf ${skiaSourceArchive} --strip-components=1 -C "$out"
    ${pkgs.lib.concatMapStringsSep "\n" (dep: ''
      mkdir -p "$out/third_party/externals/${dep.name}"
      cp -r ${dep.source}/. "$out/third_party/externals/${dep.name}/"
    '') windowsSkiaDependencies}
  '';
  # The Windows tree with the Linux-only externals, as real directories: a
  # Skia built from source writes its gn and ninja output into its own tree.
  linuxSkiaSource = pkgs.runCommand "talk-skia-0.157.0-linux-source" { } ''
    mkdir -p "$out"
    cp -r --no-preserve=mode ${windowsSkiaSource}/. "$out/"
    ${pkgs.lib.concatMapStringsSep "\n" (dep: ''
      mkdir -p "$out/third_party/externals/${dep.name}"
      cp -r --no-preserve=mode ${dep.source}/. "$out/third_party/externals/${dep.name}/"
    '') linuxSkiaDependencies}
  '';
  # Linux's Skia with Vulkan in it, built from the tree above: the renderer that
  # draws the window through a Vulkan device, which is the one device a stream's
  # DMA-BUF picture can be shown from without a copy. Its OpenGL half stays, so a
  # build that carries it still falls back to OpenGL where no device opens.
  skiaVulkan = import ./skia-linux-vulkan.nix {
    inherit pkgs;
    skiaSource = linuxSkiaSource;
  };

  libraries = with pkgs; [
    alsa-lib libpulseaudio fontconfig freetype glib libxkbcommon openssl
    wayland libGL
    # The window's Vulkan device asks for the loader at run time, and the same
    # loader is what a release wraps: it finds the card's driver through the
    # ICD files in a share dir, which is /run/opengl-driver's on NixOS and
    # /usr/share's elsewhere.
    vulkan-loader
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
