# Yapfast's Discord-only build, as the releases ship it, for the flake at the
# repository's root. `pkgs` carries rust-overlay, for the pinned toolchain.
# `commit` and `date` name what it's built from, which its build script can't
# ask Git for in the sandbox, and `release` makes it call itself a stable
# release or a nightly instead of a development build (src/build_info.rs).
# `features` are Cargo's.
{ pkgs, commit ? "", date ? "", release ? false, features ? [ "desktop" "discord" ] }:
let
  inherit (pkgs) lib;
  native = import ./native.nix { inherit pkgs; };
  toolchain = pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
  rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
  llvmPackages = pkgs.llvmPackages_23;
  id = "io.github.yapfast.Yapfast";
  # glibc 2.43 and 2.44 changed the default versions of these libm symbols.
  # Tail-call their x86-64 compatibility versions to keep the AppImage's ABI floor.
  libmSymbols = [ "acosf" "acoshf" "asinf" "atan2f" "cosh" "coshf" "log10f" "sinh" "sinhf" ];
  libmCompat = pkgs.writeText "appimage-libm.S" (''
    .text
  '' + lib.concatMapStrings (symbol: ''
    .symver compat_${symbol}, ${symbol}@GLIBC_2.2.5
    .globl __wrap_${symbol}
    .type __wrap_${symbol}, @function
    __wrap_${symbol}:
      jmp compat_${symbol}@PLT
    .size __wrap_${symbol}, .-__wrap_${symbol}
  '') libmSymbols + ''
    .section .note.GNU-stack,"",@progbits
  '');
in
rustPlatform.buildRustPackage {
  pname = "yapfast";
  inherit ((lib.importTOML ../Cargo.toml).package) version;
  # Only what the build reads, so a change to the docs or tests rebuilds nothing.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml ../Cargo.lock ../build.rs ../src ../crates ../vendor ../packaging
      # Built into yapfast-core for `yapfast theme new`.
      ../themes/template
      # A workspace member, which cargo reads even though the build leaves it out.
      ../testing/hosts/Cargo.toml ../testing/tray_host.rs ../testing/record_host.rs
    ];
  };
  cargoLock = {
    lockFile = ../Cargo.lock;
    # One crate per Git repository and commit; update a hash with the commit.
    outputHashes = {
      "const_panic-0.2.15" = "sha256-zEmo8lKncnNhn2LL5aRT95AZD/LqjmNcRg2qTKXLcic=";
      "matrix-rtc-bridge-0.4.0-rc.1" = "sha256-Zm+5xARhyrix22Ihz2fesWkwg21FaT/eqi240eT1lnQ=";
      "matrix-sdk-0.19.1" = "sha256-MzEIaDxJVssAiTd2r7qRSdkZvD+IYpC71shuJU5TATE=";
    };
  };
  buildNoDefaultFeatures = true;
  buildFeatures = features;
  nativeBuildInputs = with pkgs; [
    pkg-config cmake llvmPackages.clang llvmPackages.llvm protobuf
    # Assembles x264's x86-64 code (vendor/x264-src).
    nasm
    makeWrapper (rustPlatform.bindgenHook.override { inherit (llvmPackages) clang; })
  ];
  buildInputs = native.libraries;
  # The AppImage links the native libraries selected by Cargo, including bundled AWS-LC.
  dontCargoUseSystemLibs = true;
  # cmake is for crates' build scripts, not a configure step of its own.
  dontUseCmakeConfigure = true;
  preBuild = lib.optionalString release ''
    clang -c ${libmCompat} -o "$TMPDIR/appimage-libm.o"
    export NIX_LDFLAGS="$NIX_LDFLAGS $TMPDIR/appimage-libm.o ${lib.concatMapStringsSep " " (symbol: "--wrap=${symbol}") libmSymbols} -lm"
  '';
  env = {
    CC = "clang";
    CXX = "clang++";
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    LK_CUSTOM_WEBRTC = "${native.webrtc}";
  }
  // lib.optionalAttrs (commit != "") { YAPFAST_COMMIT = commit; }
  // lib.optionalAttrs (date != "") { YAPFAST_COMMIT_DATE = date; }
  // lib.optionalAttrs release { YAPFAST_RELEASE = "1"; };
  # The tests need a display and the fake Discord server.
  doCheck = false;
  # buildRustPackage turns off the release profile's strip and leaves it to
  # stdenv, which strips only debug info unless told otherwise; the symbol
  # tables were 55 MB of the binary.
  stripAllList = [ "bin" ];
  postInstall = ''
    install -Dm644 packaging/icon.svg "$out/share/icons/hicolor/scalable/apps/${id}.svg"
    install -Dm644 packaging/linux/${id}.desktop -t "$out/share/applications"
    install -Dm644 packaging/linux/${id}.metainfo.xml -t "$out/share/metainfo"
    # The GPU, audio and screen capture libraries are loaded at runtime, and
    # NixOS keeps the GPU driver's in /run/opengl-driver. fc-match finds the
    # system's font for a script or character the UI font lacks; the
    # system's own comes first, and NixOS may have none on its PATH.
    wrapProgram "$out/bin/yapfast" \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath native.runtimeLibraries} \
      --suffix LD_LIBRARY_PATH : /run/opengl-driver/lib \
      --suffix PATH : ${lib.makeBinPath [ pkgs.fontconfig ]}
  '';
  meta = {
    description = "Native Discord client that stays light on memory";
    homepage = "https://github.com/yapfast/yapfast";
    license = lib.licenses.gpl3Plus;
    mainProgram = "yapfast";
    platforms = [ "x86_64-linux" ];
  };
}
