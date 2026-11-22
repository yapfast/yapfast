{
  description = "Yapfast native desktop development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      native = import ./native.nix { inherit pkgs; };
      inherit (native) webrtc skia skiaPackage libraries runtimeLibraries;
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
        url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-a545de4/webrtc-win-x64-release.zip";
        hash = "sha256-hiQD/LJIx9iANIfkwiuIIcYTDokQDLnGm+le9F/0YUY=";
      };
      windowsWebrtc = pkgs.runCommand "talk-libwebrtc-windows-a545de4" {
        nativeBuildInputs = [ pkgs.unzip ];
      } ''
        unzip -q ${windowsWebrtcArchive} -d unpacked
        mv unpacked/win-x64-release "$out"
      '';
      windowsSkiaSourceArchive = pkgs.fetchurl {
        url = "https://codeload.github.com/rust-skia/skia/tar.gz/6a8fe80eadfac737c12e64229a1b9530bc78f3d1";
        sha256 = "dad0161d334409990f6f4409d1a26c1b0389e1668b79c0e175aecef651fd346d";
      };
      skiaDependency = name: url: sha256: { inherit name; source = pkgs.fetchzip { inherit url sha256; stripRoot = false; }; };
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
      # The Linux build also has Skia's own FreeType in it (nix/native.nix);
      # the Windows build draws text through DirectWrite.
      linuxSkiaDependencies = [
        (skiaDependency "freetype" "https://chromium.googlesource.com/chromium/src/third_party/freetype2/+archive/37515cf56ba3349656049edbed89ffc6f6466f2d.tar.gz" "sha256-Mcmb67mYYs6c8yQv39iIq5gxUGBZmWQXkMGSOw6jt14=")
      ];
      windowsSkiaSource = pkgs.runCommand "talk-skia-0.157.0-windows-source" {} ''
        mkdir -p "$out"
        tar -xzf ${windowsSkiaSourceArchive} --strip-components=1 -C "$out"
        ${pkgs.lib.concatMapStringsSep "\n" (dep: ''
          mkdir -p "$out/third_party/externals/${dep.name}"
          cp -r ${dep.source}/. "$out/third_party/externals/${dep.name}/"
        '') windowsSkiaDependencies}
      '';
    in {
      # scripts/package-linux.py copies the notices of Skia's third-party
      # code from here; the prebuilt Linux archive carries only Skia's own.
      packages.${system}.skia-source = pkgs.symlinkJoin {
        name = "yapfast-skia-0.157.0-linux-source";
        paths = [ windowsSkiaSource ] ++ map (dep: pkgs.runCommand "yapfast-skia-${dep.name}" {} ''
          mkdir -p "$out/third_party/externals/${dep.name}"
          cp -r ${dep.source}/. "$out/third_party/externals/${dep.name}/"
        '') linuxSkiaDependencies;
      };
      devShells.${system} = {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
          	rustup pkg-config cmake ninja llvmPackages_23.clang llvmPackages_23.libclang
           	protobuf curl jq docker-compose python315 llvmPackages_23.llvm
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
          LIBCLANG_PATH = "${pkgs.llvmPackages_23.libclang.lib}/lib";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          # webrtc-sys must not download binaries from inside a Nix build sandbox.
          LK_CUSTOM_WEBRTC = "${webrtc}";
          SKIA_BINARIES_URL = "file://${skia}/${skiaPackage}";
          shellHook = ''
            export LD_LIBRARY_PATH="/run/opengl-driver/lib:$LD_LIBRARY_PATH"
            # Only people see the greeting; commands run through the shell don't.
            [ -t 1 ] && echo "Yapfast dev shell. Run scripts/check --fast or cargo run --profile fast."
          '';
        };
        windows = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup cargoXwin cmake ninja gn protobuf python315 curl jq zip wineWowPackages.stable
            # Inno Setup's own installer opens windows even when silent.
            xvfb-run
            llvmPackages_23.clang llvmPackages_23.lld llvmPackages_23.llvm
            llvmPackages_23.libclang
            # Assembles x264's x86-64 code (vendor/x264-src).
            nasm
          ];
          CC = "clang";
          CXX = "clang++";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          LK_CUSTOM_WEBRTC = "${windowsWebrtc}";
          YAPFAST_SKIA_WINDOWS_SOURCE = "${windowsSkiaSource}";
          YAPFAST_SKIA_CLANG = "${pkgs.llvmPackages_23.clang-unwrapped}/bin/clang-cl";
          YAPFAST_SKIA_CLANG_VERSION = pkgs.lib.getVersion pkgs.llvmPackages_23.clang-unwrapped;
          YAPFAST_SKIA_LLD = "${pkgs.llvmPackages_23.lld}/bin/lld-link";
          YAPFAST_SKIA_CLANG_HEADERS = "${pkgs.llvmPackages_23.clang}/resource-root/include";
          LIBCLANG_PATH = "${pkgs.llvmPackages_23.libclang.lib}/lib";
        };
      };
    };
}
