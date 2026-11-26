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
  id = "io.github.yapfast.Yapfast";
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
      "matrix-rtc-call-0.5.0" = "sha256-PYBmi+kztQdCGw6zDusdiWgwsy3a0NX1wYjytvPETI8=";
      "matrix-sdk-0.19.1" = "sha256-5mBDl913e49ZPY/zcK3NqjuatH+6yyq8qA7yO7/9F/c=";
      "ruma-0.17.0" = "sha256-bs/aoJb9PW5vrGhbduc38dnRv3KLGxPX+jD0r3sJheo=";
    };
  };
  buildNoDefaultFeatures = true;
  buildFeatures = features;
  nativeBuildInputs = with pkgs; [
    pkg-config cmake llvmPackages_21.clang llvmPackages_21.llvm protobuf
    # Assembles x264's x86-64 code (vendor/x264-src).
    nasm
    makeWrapper rustPlatform.bindgenHook
  ];
  buildInputs = native.libraries;
  # cmake is for crates' build scripts, not a configure step of its own.
  dontUseCmakeConfigure = true;
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
