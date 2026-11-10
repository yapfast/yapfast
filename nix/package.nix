# Yapfast's Discord-only build, as the releases ship it, for the flake at the
# repository's root. `pkgs` carries rust-overlay, for the pinned toolchain.
# `commit` and `date` name what it's built from, which its build script can't
# ask Git for in the sandbox, and `release` makes it call itself a stable
# release or a nightly instead of a development build (src/build_info.rs).
{ pkgs, commit ? "", date ? "", release ? false }:
let
  inherit (pkgs) lib;
  native = import ./native.nix { inherit pkgs; };
  toolchain = pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
  rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
  id = "io.github.yapfast.Yapfast";
  deepFilterHash = "sha256-JxvK5/PX29o23Pq9cNpTSyuAB7j9HszzDGHC2fOw74Y=";
  # deep_filter embeds a model from its repository's models/ folder, outside
  # the crate, which vendoring leaves out; this is the commit Cargo.lock pins.
  deepFilter =
    let
      lock = lib.importTOML ../Cargo.lock;
      source = (lib.findFirst (p: p.name == "deep_filter") null lock.package).source;
    in
    pkgs.fetchgit {
      url = "https://github.com/Rikorose/DeepFilterNet";
      rev = lib.last (lib.splitString "#" source);
      hash = deepFilterHash;
    };
in
rustPlatform.buildRustPackage {
  pname = "yapfast";
  inherit ((lib.importTOML ../Cargo.toml).package) version;
  # Only what the build reads, so a change to the docs or tests rebuilds nothing.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml ../Cargo.lock ../build.rs ../src ../crates ../ui ../translations ../vendor ../packaging
    ];
  };
  cargoLock = {
    lockFile = ../Cargo.lock;
    # One crate per Git repository and commit; update a hash with the commit.
    outputHashes = {
      "const_panic-0.2.15" = "sha256-zEmo8lKncnNhn2LL5aRT95AZD/LqjmNcRg2qTKXLcic=";
      "deep_filter-0.5.7-pre" = deepFilterHash;
      "matrix-rtc-bridge-0.4.0-rc.1" = "sha256-Zm+5xARhyrix22Ihz2fesWkwg21FaT/eqi240eT1lnQ=";
      "matrix-sdk-0.19.1" = "sha256-MzEIaDxJVssAiTd2r7qRSdkZvD+IYpC71shuJU5TATE=";
      "slint-1.19.0" = "sha256-gAu/zH5z6UrlEolvzpmcjsFZlJG2PWV96nNRgrdKyFs=";
    };
  };
  buildNoDefaultFeatures = true;
  buildFeatures = [ "desktop" "discord" ];
  nativeBuildInputs = with pkgs; [
    pkg-config cmake llvmPackages_21.clang llvmPackages_21.llvm protobuf
    # Assembles x264's x86-64 code (vendor/x264-src).
    nasm
    # Skia's build script fetches its archive with curl, even from a file:// URL.
    curl makeWrapper rustPlatform.bindgenHook
  ];
  buildInputs = native.libraries;
  # cmake is for crates' build scripts, not a configure step of its own.
  dontUseCmakeConfigure = true;
  env = {
    CC = "clang";
    CXX = "clang++";
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    LK_CUSTOM_WEBRTC = "${native.webrtc}";
    SKIA_BINARIES_URL = "file://${native.skia}/${native.skiaPackage}";
  }
  // lib.optionalAttrs (commit != "") { YAPFAST_COMMIT = commit; }
  // lib.optionalAttrs (date != "") { YAPFAST_COMMIT_DATE = date; }
  // lib.optionalAttrs release { YAPFAST_RELEASE = "1"; };
  preBuild = ''
    chmod u+w "$NIX_BUILD_TOP/cargo-vendor-dir"
    install -Dm644 ${deepFilter}/models/DeepFilterNet3_onnx.tar.gz -t "$NIX_BUILD_TOP/cargo-vendor-dir/models"
  '';
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
    # NixOS keeps the GPU driver's in /run/opengl-driver.
    wrapProgram "$out/bin/yapfast" \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath native.runtimeLibraries} \
      --suffix LD_LIBRARY_PATH : /run/opengl-driver/lib
  '';
  meta = {
    description = "Native Discord client that stays light on memory";
    homepage = "https://github.com/yapfast/yapfast";
    license = lib.licenses.gpl3Plus;
    mainProgram = "yapfast";
    platforms = [ "x86_64-linux" ];
  };
}
