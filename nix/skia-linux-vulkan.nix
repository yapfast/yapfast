# Linux's Skia with Vulkan, built from source: rust-skia's prebuilt Linux
# archives have no Vulkan beside Skia's own FreeType, and Yapfast needs that
# FreeType for its colour emoji. The OpenGL half stays in it, so a build can
# still fall back to OpenGL. `prepared` is what SKIA_BINARIES_URL points at;
# nix/skia-linux-builder names the features, nix/prepare-skia-native.py the
# JPEG table's rename. Windows' own source build is nix/prepare-windows-skia.py.
{ pkgs, skiaSource }:
let
  key = "30669978c3f39a73e8a7-x86_64-unknown-linux-gnu-ftembed-ganesh-gl-jpegd-jpege-pdf-textlayout-vulkan";
  package = "skia-binaries-${key}.tar.gz";
  # The archive skia-bindings exports, before prepare-skia-native.py renames the
  # JPEG table: the pin is its digest, so a build that stops producing exactly
  # these bytes fails here instead of shipping them. prepare-skia-native.py
  # carries the same digest for the flavour it names `linux-vulkan`.
  rawSha256 = "sha256-DAjIWrRrB+Jkob/bISoN54k4sNNFkTShlXtxElaou3g=";
  raw = pkgs.rustPlatform.buildRustPackage {
    pname = "yapfast-skia-vulkan-raw";
    version = "0.157.0";
    src = ./skia-linux-builder;
    cargoLock.lockFile = ./skia-linux-builder/Cargo.lock;
    nativeBuildInputs = with pkgs; [
      clang gn ninja python3 llvmPackages_21.llvm pkg-config
    ];
    # Skia's font manager reads fontconfig, and its GL backend gl.h.
    buildInputs = with pkgs; [ fontconfig libGL ];
    LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
    # Skia's build passes `--target=x86_64-linux-gnu`, which clang takes; the
    # buildPhase exports them too, where the stdenv's gcc-wrapper has set CC.
    CC = "clang";
    CXX = "clang++";
    outputHash = rawSha256;
    outputHashMode = "flat";
    outputHashAlgo = "sha256";
    buildPhase = ''
      runHook preBuild
      export CC=clang CXX=clang++
      # The pinned source is read-only, and Skia's gn and ninja write into it.
      cp -r --no-preserve=mode ${skiaSource} skia
      export SKIA_SOURCE_DIR="$PWD/skia"
      export SKIA_GN_COMMAND=${pkgs.gn}/bin/gn
      printf '#!/bin/sh\nexec %s -j6 "$@"\n' ${pkgs.ninja}/bin/ninja > ninja
      chmod +x ninja
      export SKIA_NINJA_COMMAND="$PWD/ninja"
      cargo build --release --offline
      runHook postBuild
    '';
    installPhase = ''
      # skia-bindings leaves the built Skia in its build script's out directory.
      skia_out=$(find target/release/build -maxdepth 4 -type d -path '*skia-bindings-*/out/skia' | head -1)
      python3 ${./skia-linux-vulkan-archive.py} "$skia_out" "$PWD/skia/LICENSE" \
        "${key}" "$out"
    '';
  };
in {
  inherit key package raw;
  prepared = pkgs.runCommand "talk-skia-0.157.0-linux-vulkan" {
    nativeBuildInputs = [ pkgs.python3 pkgs.binutils pkgs.llvmPackages_21.llvm ];
    YAPFAST_SKIA_SOURCE = raw;
    YAPFAST_SKIA_TARGET = "linux-vulkan";
    YAPFAST_SKIA_OBJCOPY = "${pkgs.llvmPackages_21.llvm}/bin/llvm-objcopy";
    YAPFAST_SKIA_NM = "${pkgs.llvmPackages_21.llvm}/bin/llvm-nm";
  } ''
    export YAPFAST_SKIA_OUTPUT="$out"
    python3 ${./prepare-skia-native.py}
  '';
}
