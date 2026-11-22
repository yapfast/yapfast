# The Linux release's AppImage, before scripts/package-linux.py adds the
# licenses and packs it: nix/package.nix's binary in an AppDir, and the
# AppImage runtime that goes in front of it.
#
# The binary runs on the host's dynamic loader and glibc rather than Nix's, as
# an AppImage built on a distribution would, so the host's own GPU driver,
# Mesa or NVIDIA's, loads into it. Nothing else is bundled: it links only
# libraries every desktop has, and loads the GPU, display, audio and screen
# capture ones at runtime. The glibc it was built against sets the oldest
# distribution it runs on.
{ pkgs, yapfast }:
let
  id = "io.github.yapfast.Yapfast";
  native = import ./native.nix { inherit pkgs; };
  # The newest glibc and libstdc++ the binary may ask for: Ubuntu 24.04's and
  # Fedora 40's. Nix builds against a newer glibc, so a nixpkgs update can
  # raise what the binary asks for, and this fails the build when it does.
  glibcFloor = "2.39";
  libstdcxxFloor = "3.4.33";
  # Libraries every desktop distribution installs, so the binary may link them.
  hostLibraries = [
    "ld-linux-x86-64.so.2" "libc.so.6" "libm.so.6" "libgcc_s.so.1" "libstdc++.so.6"
    "libglib-2.0.so.0" "libgobject-2.0.so.0" "libgio-2.0.so.0"
    "libpulse.so.0" "libpulse-simple.so.0" "libfontconfig.so.1" "libfreetype.so.6"
  ];
  # The official runtime, statically linked: it needs FUSE 3's fusermount3
  # but not libfuse, and runs with --appimage-extract-and-run without either.
  runtime = pkgs.fetchurl {
    url = "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64";
    sha256 = "0396xi3km1yd0z4329lbjxmv82ddc40gd0x8hca0ylcj7i28pjig";
  };
  # Where AppImageUpdate and the AppImage managers look for a newer release.
  updateInformation = "gh-releases-zsync|yapfast|yapfast|latest|Yapfast-*-x86_64.AppImage.zsync";
  # What scripts/package-linux.py adds to the release's BUILD.json.
  build = builtins.toJSON {
    glibc_floor = glibcFloor;
    libstdcxx_floor = libstdcxxFloor;
    appimage_runtime = runtime.url;
    webrtc_archive = native.webrtcArchive.url;
    webrtc_archive_hash = native.webrtcArchive.outputHash;
    skia_archive = native.skiaArchive.url;
    skia_archive_hash = native.skiaArchive.outputHash;
    update_information = updateInformation;
  };
in
pkgs.runCommand "yapfast-appdir-${yapfast.version}" {
  nativeBuildInputs = with pkgs; [ patchelf binutils librsvg ];
  inherit (yapfast) version;
  inherit updateInformation build;
} ''
  appdir="$out/AppDir"
  mkdir -p "$out"
  printf '%s\n' "$build" > "$out/build.json"
  mkdir -p "$appdir/usr/bin"
  # wrapProgram's wrapper sets Nix's library path; the binary itself is beside it.
  cp ${yapfast}/bin/.yapfast-wrapped "$appdir/usr/bin/yapfast"
  chmod u+w "$appdir/usr/bin/yapfast"
  patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$appdir/usr/bin/yapfast"

  needed=$(readelf -d "$appdir/usr/bin/yapfast" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p')
  for library in $needed; do
    case " ${toString hostLibraries} " in
      *" $library "*) ;;
      *) echo "yapfast links $library, which not every distribution has; bundle it or load it at runtime" >&2; exit 1 ;;
    esac
  done
  newest() {
    readelf -V "$appdir/usr/bin/yapfast" | grep -o "Name: $1_[0-9.]*" | sed "s/Name: $1_//" | sort -uV | tail -1
  }
  for pair in GLIBC:${glibcFloor} GLIBCXX:${libstdcxxFloor}; do
    name=''${pair%%:*} floor=''${pair#*:} asked=$(newest "''${pair%%:*}")
    if [ "$(printf '%s\n' "$asked" "$floor" | sort -V | tail -1)" != "$floor" ]; then
      echo "yapfast asks for $name $asked, newer than the $floor the AppImage promises" >&2
      exit 1
    fi
  done

  share=${yapfast}/share
  cp -r "$share" "$appdir/usr/share"
  chmod -R u+w "$appdir/usr/share"
  ln -s usr/bin/yapfast "$appdir/AppRun"
  ln -s usr/share/applications/${id}.desktop "$appdir/${id}.desktop"
  ln -s usr/share/icons/hicolor/scalable/apps/${id}.svg "$appdir/${id}.svg"
  # File managers show .DirIcon as the AppImage's thumbnail, and want a PNG.
  rsvg-convert -w 256 -h 256 "$share/icons/hicolor/scalable/apps/${id}.svg" -o "$appdir/.DirIcon"

  # The runtime keeps a section for the update information.
  install -m755 ${runtime} "$out/runtime"
  read -r offset size < <(readelf -SW "$out/runtime" |
    sed -n 's/^ *\[ *[0-9]*\] //p' | awk '$1 == ".upd_info" { print $4, $5 }')
  [ ''${#updateInformation} -lt $((16#$size)) ] || { echo "update information too long" >&2; exit 1; }
  printf '%s' "$updateInformation" | dd of="$out/runtime" bs=1 seek=$((16#$offset)) conv=notrunc status=none
''
