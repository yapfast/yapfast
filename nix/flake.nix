{
  description = "Yapfast native desktop development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      native = import ./native.nix { inherit pkgs; };
      inherit (native) webrtc libraries runtimeLibraries;
      llvmPackages = pkgs.llvmPackages_23;
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
      devShells.${system} = {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup pkg-config cmake ninja llvmPackages.clang llvmPackages.libclang
            protobuf curl jq docker-compose python3 llvmPackages.llvm
            # pasta, nftables and iproute2 isolate one voice test participant's network.
            passt nftables iproute2
            # Runs testing/fake_discord.ts, and its tests side by side.
            bun cargo-nextest
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          buildInputs = libraries;
          # mold through the Nix ld wrapper, which puts the directories of the
          # libraries it links (GLib, PulseAudio, libstdc++) in the binary's
          # runpath, so a build also runs outside this shell.
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = pkgs.writeShellScript "yapfast-linker" ''
            exec ${llvmPackages.clang}/bin/clang --ld-path=${pkgs.mold}/bin/ld.mold "$@"
          '';
          # The libraries Yapfast opens at run time, such as the Vulkan loader,
          # which no runpath names.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ]);
          CC = "clang";
          CXX = "clang++";
          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          # webrtc-sys must not download binaries from inside a Nix build sandbox.
          LK_CUSTOM_WEBRTC = "${webrtc}";
          shellHook = ''
            export LD_LIBRARY_PATH="/run/opengl-driver/lib:$LD_LIBRARY_PATH"
            # Only people see the greeting; commands run through the shell don't.
            [ -t 1 ] && echo "Yapfast dev shell. Run scripts/check --fast or cargo run --profile fast."
          '';
        };
        windows = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup cargo-xwin cmake ninja protobuf python3 curl jq zip wineWow64Packages.stable
            # Inno Setup's own installer opens windows even when silent.
            xvfb-run
            llvmPackages.clang llvmPackages.lld llvmPackages.llvm
            llvmPackages.libclang
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          CC = "clang";
          CXX = "clang++";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          LK_CUSTOM_WEBRTC = "${windowsWebrtc}";
          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
        };
      };
    };
}
