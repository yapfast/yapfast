{
  description = "Yapfast native desktop development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      native = import ./native.nix { inherit pkgs; };
      inherit (native) webrtc libraries runtimeLibraries;
      # nixos-25.11's cargo-xwin 0.20.2 has xwin 0.6.7, which finds no headers
      # for the Windows SDK in Visual Studio's 17.14.45 manifest: it names
      # them `Installers/…` where older ones had `Installers\…`. xwin 0.10.0
      # takes both. A machine that downloaded the SDK before keeps its copy.
      cargoXwin = pkgs.cargo-xwin.overrideAttrs (old: rec {
        version = "0.23.1";
        src = pkgs.fetchFromGitHub {
          owner = "rust-cross";
          repo = "cargo-xwin";
          tag = "v${version}";
          hash = "sha256-JQYAYCCN/dWvX1oqFX/MjvtPuCA3k8WbHmmGBxG9ylA=";
        };
        cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          inherit src;
          name = "cargo-xwin-${version}";
          hash = "sha256-FRK4bCTPAPWGov8vEFr9XdqCNoeXeyFdyiWV5x+4WYY=";
        };
      });
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
            rustup pkg-config cmake ninja llvmPackages_21.clang llvmPackages_21.libclang
            protobuf curl jq docker-compose python3 llvmPackages_21.llvm
            # pasta, nftables and iproute2 isolate one voice test participant's network.
            passt nftables iproute2
            # Runs testing/fake_discord.ts, and its tests side by side.
            bun cargo-nextest
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          buildInputs = libraries;
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = pkgs.writeShellScript "yapfast-linker" ''
            exec ${pkgs.llvmPackages_21.clang}/bin/clang --ld-path=${pkgs.mold}/bin/mold "$@"
          '';
          # Keep native libraries available when the linker bypasses the Nix
          # ld wrapper that would add their directories to the binary's runpath.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ]);
          CC = "clang";
          CXX = "clang++";
          LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
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
            rustup cargoXwin cmake ninja protobuf python3 curl jq zip wineWowPackages.stable
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
          LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
        };
      };
    };
}
