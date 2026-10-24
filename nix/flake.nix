{
  description = "Yapfast native desktop development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      native = import ./native.nix { inherit pkgs; };
      inherit (native) webrtc skia skiaPackage libraries;
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
      windowsSkiaSourceArchive = pkgs.fetchurl {
        url = "https://codeload.github.com/rust-skia/skia/tar.gz/61e7ca4e99062cdd0ab69445d5963fb3365778f6";
        sha256 = "8401d0de878582a03abc1029bb62330aea58cda89904ae6ab187302e6d48ba5d";
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
        (skiaDependency "expat" "https://chromium.googlesource.com/external/github.com/libexpat/libexpat/+archive/6154446fccefbf3ca644894f598969113b0c7bcd.tar.gz" "sha256-V8mzuJJj7kO24YNN7nBL913lzB911QUvy0CJzsTcF90=")
        (skiaDependency "libjpeg-turbo" "https://chromium.googlesource.com/chromium/deps/libjpeg_turbo/+archive/e14cbfaa85529d47f9f55b0f104a579c1061f9ad.tar.gz" "sha256-Ig+tmprZDvlf/M72/DTar2pbxat9ZElgSqdXdoM0lPs=")
        (skiaDependency "libpng" "https://skia.googlesource.com/third_party/libpng/+archive/d5515b5b8be3901aac04e5bd8bd5c89f287bcd33.tar.gz" "sha256-N507gFyjXXsJ3dyQN0SlVy80WXlbNlZs/cnqLaCmFTk=")
        (skiaDependency "wuffs" "https://skia.googlesource.com/external/github.com/google/wuffs-mirror-release-c/+archive/e3f919ccfe3ef542cfc983a82146070258fb57f8.tar.gz" "sha256-373d2F/STcgCHEq+PO+SCHrKVOo6uO1rqqwRN5eeBCw=")
        (skiaDependency "zlib" "https://chromium.googlesource.com/chromium/src/third_party/zlib/+archive/646b7f569718921d7d4b5b8e22572ff6c76f2596.tar.gz" "sha256-jNj6SuTZ5/a7crtYhxW3Q/TlfRMNMfYIVxDlr7bYdzQ=")
        (skiaDependency "spirv-cross" "https://chromium.googlesource.com/external/github.com/KhronosGroup/SPIRV-Cross/+archive/b8fcf307f1f347089e3c46eb4451d27f32ebc8d3.tar.gz" "sha256-H43M9DXfEuyKuvo6rjb5k0KEbYOSFodbPJh8ZKY4PQg=")
      ];
      # The Linux build also has Skia's own FreeType in it (nix/native.nix);
      # the Windows build draws text through DirectWrite.
      linuxSkiaDependencies = [
        (skiaDependency "freetype" "https://chromium.googlesource.com/chromium/src/third_party/freetype2/+archive/264b5fbf5b912b39f98d038bf75d39be0a73f21b.tar.gz" "sha256-UeV2TAEDylx72nRVrzIVquzfduHgHWvO2Lw+OTWZrqE=")
      ];
      windowsSkiaSource = pkgs.runCommand "talk-skia-0.153.3-windows-source" {} ''
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
        name = "yapfast-skia-0.153.3-linux-source";
        paths = [ windowsSkiaSource ] ++ map (dep: pkgs.runCommand "yapfast-skia-${dep.name}" {} ''
          mkdir -p "$out/third_party/externals/${dep.name}"
          cp -r ${dep.source}/. "$out/third_party/externals/${dep.name}/"
        '') linuxSkiaDependencies;
      };
      devShells.${system} = {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup pkg-config cmake ninja llvmPackages_21.clang llvmPackages_21.libclang
            protobuf curl jq docker-compose python3 llvmPackages_21.llvm
            # pasta, nftables and iproute2 isolate one voice test participant's network.
            passt nftables iproute2
            # Runs testing/fake_discord.ts.
            bun
          ];
          buildInputs = libraries;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libraries;
          CC = "clang";
          CXX = "clang++";
          LIBCLANG_PATH = "${pkgs.llvmPackages_21.libclang.lib}/lib";
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
            rustup cargo-xwin cmake ninja gn protobuf python3 curl jq zip wineWowPackages.stable
            # Inno Setup's own installer opens windows even when silent.
            xvfb-run
            llvmPackages_21.clang llvmPackages_21.lld llvmPackages_21.llvm
            llvmPackages_21.libclang
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
