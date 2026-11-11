{
  description = "Yapfast native desktop development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      native = import ./native.nix { inherit pkgs; };
      inherit (native) webrtc skia skiaPackage libraries runtimeLibraries skiaVulkan
        windowsSkiaSource linuxSkiaSource;
      windowsWebrtcArchive = pkgs.fetchurl {
        url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-51ef663/webrtc-win-x64-release.zip";
        hash = "sha256-ClalyRs7e0IiCCuKCTiNFqgCzeItZylb6yTFVwB1WzA=";
      };
      windowsWebrtc = pkgs.runCommand "talk-libwebrtc-windows-51ef663" {
        nativeBuildInputs = [ pkgs.unzip ];
      } ''
        unzip -q ${windowsWebrtcArchive} -d unpacked
        mv unpacked/win-x64-release "$out"
      '';
    in {
      packages.${system} = {
        # scripts/package-linux.py copies the notices of Skia's third-party
        # code from here; the prebuilt Linux archive carries only Skia's own.
        skia-source = linuxSkiaSource;
        skia = skia;
        # Linux's Skia with Vulkan in it, for `SKIA_BINARIES_URL`; building it
        # takes a few minutes of Skia, once per change to
        # nix/skia-linux-vulkan.nix.
        skia-vulkan = skiaVulkan.prepared;
        skia-vulkan-raw = skiaVulkan.raw;
      };
      devShells.${system} = {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup pkg-config cmake ninja llvmPackages_21.clang llvmPackages_21.libclang
            # Builds the Skia source a Vulkan build needs, with ninja; see
            # nix/prepare-linux-skia.py.
            gn
            protobuf curl jq docker-compose python3 llvmPackages_21.llvm
            # pasta, nftables and iproute2 isolate one voice test participant's network.
            passt nftables iproute2
            # Runs testing/fake_discord.ts, and its tests side by side.
            bun cargo-nextest
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          buildInputs = libraries;
          # rustc links with its own rust-lld, which skips the Nix ld wrapper
          # that would put libstdc++'s directory in the binary's runpath.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ]);
          CC = "clang";
          CXX = "clang++";
          LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          # webrtc-sys must not download binaries from inside a Nix build sandbox.
          LK_CUSTOM_WEBRTC = "${webrtc}";
          # Every Linux build carries Skia's Vulkan, which is the archive with
          # Vulkan in it here: the window draws on a Vulkan device where the
          # machine opens one and on OpenGL otherwise, and the archive holds
          # both. Building it takes a few minutes of Skia once,
          # `nix build path:./nix#skia-vulkan`.
          SKIA_BINARIES_URL = "file://${skiaVulkan.prepared}/${skiaVulkan.package}";
          # The same Skia without Vulkan, for a build that asks for it by
          # leaving the `skia-vulkan` feature out: skia-safe keys its archive by
          # the features it is asked for, so such a build wants this one:
          # `SKIA_BINARIES_URL=$SKIA_GL_BINARIES_URL cargo check --profile fast
          # --no-default-features --features desktop,discord`.
          SKIA_GL_BINARIES_URL = "file://${skia}/${skiaPackage}";
          shellHook = ''
            export LD_LIBRARY_PATH="/run/opengl-driver/lib:$LD_LIBRARY_PATH"
            # Only people see the greeting; commands run through the shell don't.
            [ -t 1 ] && echo "Yapfast dev shell. Run scripts/check --fast or cargo run --profile fast."
          '';
        };
        windows = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup cargo-xwin cmake ninja gn protobuf python3 curl jq zip wineWowPackages.stable
            # Inno Setup's own installer opens windows even when silent.
            xvfb-run
            llvmPackages_21.clang llvmPackages_21.lld llvmPackages_21.llvm
            llvmPackages_21.libclang
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          CC = "clang";
          CXX = "clang++";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          LK_CUSTOM_WEBRTC = "${windowsWebrtc}";
          YAPFAST_SKIA_WINDOWS_SOURCE = "${windowsSkiaSource}";
          YAPFAST_SKIA_CLANG = "${pkgs.llvmPackages_21.clang-unwrapped}/bin/clang-cl";
          YAPFAST_SKIA_LLD = "${pkgs.llvmPackages_21.lld}/bin/lld-link";
          YAPFAST_SKIA_CLANG_HEADERS = "${pkgs.llvmPackages_21.clang}/resource-root/include";
          LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
        };
      };
    };
}
