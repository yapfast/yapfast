#!/usr/bin/env bash
# Regenerates ffmpeg/, include/ and gen/ from pinned sources: FFmpeg's
# release, nv-codec-headers, AMD's AMF headers, Intel's oneVPL, and libva's
# headers from the development shell's nixpkgs. It configures and builds
# FFmpeg once for each target, with its assembly, the way build.rs will, and
# keeps only the files that build read, the files configure generated, and the
# list of symbols to rename; then it applies patches/ to the kept files.
# Windows is built with clang-cl against the MSVC runtime and Windows SDK
# cargo-xwin downloads, which a Windows build (scripts/build-windows) fetches
# the first time.
# Usage: vendor/ffmpeg-src/update.sh (needs the network and Nix).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)

FFMPEG_VERSION=9.0.2
FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
# 12.0 needs NVIDIA's driver 520 or newer; newer headers raise that.
NVH_VERSION=12.0.16.1
NVH_SHA256=bfb3306f22c169fedcfc5489b344f414089eec796ffd528412677b0c872bc6e2
# The oldest FFmpeg takes: the driver's AMF runtime must be as new.
AMF_VERSION=1.5.2
AMF_SHA256=d3c12eb324edf05e214608b6a395a51dd95770ed9d45520185d6c3a206811c99
# Intel's oneVPL, the dispatcher the QSV encoders call: FFmpeg 9 asks for 2.6
# or newer, and 2.14 is what it builds against. Its headers are what makes QSV
# av1_qsv, so its version also picks the codecs the encoders take. The tarball
# is GitHub's for the tag, which is the only one Intel publishes.
VPL_VERSION=2.14.0
VPL_SHA256=7c6bff1c1708d910032c2e6c44998ffff3f5fdbf06b00972bc48bf2dd9e5ac06
# Hardware encoders, and every codec Yapfast streams: the decoders and parsers
# are what a watched stream's pictures come from, in software and, on Windows,
# through Direct3D 11's own decode, on any vendor's card. libwebrtc's FFmpeg
# carries none of them, so these are the only ones in the libraries. QSV is
# Intel's, on either target, and asks oneVPL for the driver.
LINUX_ENCODERS=h264_nvenc,hevc_nvenc,av1_nvenc,h264_vaapi,hevc_vaapi,av1_vaapi,h264_qsv,hevc_qsv,av1_qsv
WINDOWS_ENCODERS=h264_nvenc,hevc_nvenc,av1_nvenc,h264_amf,hevc_amf,av1_amf,h264_qsv,hevc_qsv,av1_qsv
# H264, HEVC, AV1 and VP9 are what Discord negotiates, in a call and in a
# stream, and the four decoders here are also the build's software path.
DECODERS=h264,hevc,av1,vp9
PARSERS=h264,hevc,av1,vp9
# The hardware decode hands its surfaces to the device the window and the
# encoder already hold, through AVD3D11VAFramesContext, whose pool decoder.rs
# allocates up front. The *_d3d11va2 variants decode into textures the caller
# owns; these give frames the caller can pre-allocate.
WINDOWS_HWACCELS=h264_d3d11va,hevc_d3d11va,av1_d3d11va,vp9_d3d11va
# The same decode on Linux, through VA-API: FFmpeg leaves each picture in a
# VA surface, and libdrm is what lets av_hwframe_map hand it over as a
# DMA-BUF (`AV_PIX_FMT_DRM_PRIME`) instead of copying it back.
LINUX_HWACCELS=h264_vaapi,hevc_vaapi,av1_vaapi,vp9_vaapi
COMMON=(--disable-everything --disable-programs --disable-doc --disable-network
  --disable-autodetect --disable-avformat --disable-avdevice --disable-swscale
  --disable-swresample --disable-avfilter --disable-iconv
  --disable-debug --enable-pic --enable-static --disable-shared)

if [[ -z "${YAPFAST_FFMPEG_UPDATE:-}" ]]; then
  export YAPFAST_FFMPEG_UPDATE=1
  exec nix shell --inputs-from "path:$root/nix" nixpkgs#gnumake nixpkgs#clang \
    nixpkgs#pkg-config nixpkgs#binutils nixpkgs#nasm nixpkgs#curl nixpkgs#gnutar nixpkgs#xz nixpkgs#patch \
    nixpkgs#cmake nixpkgs#gcc \
    --command "$0" "$@"
fi
xwin=${XWIN_CACHE_DIR:-$HOME/.cache/cargo-xwin}/xwin
if [[ ! -d "$xwin/sdk" ]]; then
  echo "update.sh: no Windows SDK in $xwin; run scripts/build-windows once" >&2
  exit 1
fi
# The Windows tools, by path, so the Linux build keeps the wrapped clang.
llvm=$(nix build --no-link --print-out-paths --inputs-from "path:$root/nix" \
  nixpkgs#llvmPackages_21.clang-unwrapped nixpkgs#llvmPackages_21.lld nixpkgs#llvmPackages_21.llvm |
  tr '\n' ' ')

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fetch() {  # url file sha256
  # ffmpeg.org and the release pages reset a connection now and then.
  curl -sSfL --retry 8 --retry-delay 3 --retry-all-errors -o "$work/$2" "$1"
  echo "$3  $work/$2" | sha256sum -c --quiet
}
fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" ffmpeg.tar.xz "$FFMPEG_SHA256"
fetch "https://github.com/FFmpeg/nv-codec-headers/releases/download/n$NVH_VERSION/nv-codec-headers-$NVH_VERSION.tar.gz" \
  nvh.tar.gz "$NVH_SHA256"
fetch "https://github.com/GPUOpen-LibrariesAndSDKs/AMF/releases/download/v$AMF_VERSION/AMF-headers-v$AMF_VERSION.tar.gz" \
  amf.tar.gz "$AMF_SHA256"
# GitHub's tarball of a tag has no release to pin but the tag itself.
fetch "https://github.com/intel/libvpl/archive/refs/tags/v$VPL_VERSION.tar.gz" vpl.tar.gz \
  "$VPL_SHA256"
tar xf "$work/ffmpeg.tar.xz" -C "$work"
tar xf "$work/nvh.tar.gz" -C "$work"
mkdir "$work/amf"
tar xf "$work/amf.tar.gz" -C "$work/amf" --strip-components=1
mkdir "$work/vpl"
tar xf "$work/vpl.tar.gz" -C "$work/vpl" --strip-components=1
src=$work/ffmpeg-$FFMPEG_VERSION
make -s -C "$work/nv-codec-headers-$NVH_VERSION" PREFIX="$work/nvh" install
libva=$(nix build --no-link --print-out-paths --inputs-from "path:$root/nix" nixpkgs#libva.dev)
# configure's --enable-libvpl links a test against oneVPL, so each target needs
# one to link: the Linux build's own for Linux, and for Windows an import
# library for vpl.dll. Neither is shipped, and neither lands in config.h:
# src/vpl.c is what the build's calls go through, and it loads oneVPL's own
# library at run time, as src/va.c does libva's.
cmake -S "$work/vpl" -B "$work/vpl-build" -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TOOLS=OFF -DBUILD_EXAMPLES=OFF -DBUILD_TESTS=OFF >/dev/null
cmake --build "$work/vpl-build" -j"$(nproc)" >/dev/null
mkdir -p "$work/vpl-win/lib"
# pkg-config as configure reads it, naming only the library: oneVPL's own
# vpl.pc asks for libstdc++ as well, which this build doesn't link.
vpl_pc() {  # target, where that library is
  mkdir -p "$work/vpl-pc-$1"
  printf '%s\n' 'Name: Intel oneVPL, for QSV' 'Description: the QSV dispatcher, linked by configure only' \
    "Version: ${VPL_VERSION#v}" "Libs: -L$2 -lvpl" \
    "Cflags: -I$work/vpl/api -I$work/vpl/api/vpl" > "$work/vpl-pc-$1/vpl.pc"
}
vpl_pc linux "$work/vpl-build"
vpl_pc windows "$work/vpl-win/lib"
export PKG_CONFIG_PATH=$work/nvh/lib/pkgconfig:$libva/lib/pkgconfig:$work/vpl-pc-linux

# One target: configure, build, and record what it compiled and generated.
build() {  # target, nm, then configure's arguments
  local target=$1 nm=$2 out=$work/build-$1
  shift 2
  mkdir -p "$out"
  (cd "$out" && bash "$src/configure" "${COMMON[@]}" "$@" >/dev/null)
  make -s -C "$out" -j"$(nproc)" >/dev/null
  local gen=$work/gen/$target
  mkdir -p "$gen/libavutil" "$gen/libavcodec"
  # config_components.asm holds the codecs' switches for the assembly that
  # asks, which the h264 asm does.
  cp "$out/config.h" "$out/config_components.h" "$out/config.asm" \
    "$out/config_components.asm" "$gen/"
  cp "$out/libavutil/avconfig.h" "$out/libavutil/ffversion.h" "$gen/libavutil/"
  cp "$out"/libavcodec/{codec,parser,bsf}_list.c "$gen/libavcodec/"
  # Each object came from C or from NASM's assembly.
  (cd "$out" && find libavcodec libavutil -name '*.o' | sed 's/\.o$//') > "$work/objects.txt"
  local object
  while read -r object; do
    if [[ -f "$src/$object.asm" ]]; then echo "$object.asm"; fi
  done < "$work/objects.txt" | sort > "$gen/asm.txt"
  # intmath.c only gives av_log2 and av_log2_16bit out-of-line bodies for
  # FFmpeg's shared libraries, under #undef, which would drop their prefix;
  # FFmpeg's own code reaches them through intmath.h's macros.
  while read -r object; do
    if [[ ! -f "$src/$object.asm" ]]; then echo "$object.c"; fi
  done < "$work/objects.txt" | grep -vx libavutil/intmath.c | sort > "$gen/sources.txt"
  # Every file the compiler read, as the .d files name them, and the files
  # it compiled, which clang-cl's leave out; clang-cl's give the full path
  # of a header found beside the file including it.
  find "$out/libavcodec" "$out/libavutil" -name '*.d' -exec cat {} + | tr ' \\' '\n\n' |
    grep -v ':$' | grep . | sed "s|^$src/|src/|" >> "$work/read.txt"
  sed 's|^|src/|' "$gen/sources.txt" "$gen/asm.txt" >> "$work/read.txt"
  # What build.rs adds to compile and link as configure would: its defines,
  # its include folders inside FFmpeg, and the system libraries it links.
  sed -n 's/^CPPFLAGS=//p' "$out/ffbuild/config.mak" | tr ' ' '\n' | sed -n 's/^-D//p' > "$gen/defines.txt"
  sed -n 's/^CPPFLAGS=//p' "$out/ffbuild/config.mak" | tr ' ' '\n' |
    sed -n 's|^-I\$(SRC_PATH)/||p' > "$gen/includes.txt"
  # NASM's: the object format.
  sed -n 's/^X86ASMFLAGS=//p' "$out/ffbuild/config.mak" | tr ' ' '\n' | grep . > "$gen/asmflags.txt"
  # Not libva or oneVPL, which src/va.c and src/vpl.c load when they're used,
  # nor libatomic, which x86-64 doesn't need and which would be one more
  # library to install; nor clang-cl's -libpath: to the import library that
  # only configure's test linked, which is no folder of this machine.
  sed -n 's/^EXTRALIBS\(-avcodec\|-avutil\)\{0,1\}=//p' "$out/ffbuild/config.mak" | tr ' ' '\n' |
    sed -n -e '/^-[Ll]ibpath:/d' -e 's/^-l//p' -e 's/\.lib$//p' -e 's/^-pthread$/pthread/p' |
    grep -vxE 'va|va-drm|vpl|atomic' | sort -u > "$gen/libs.txt"
  # Archives as on Linux, libraries as on Windows.
  local libraries=() library
  for library in libavcodec/libavcodec.a libavutil/libavutil.a libavcodec/avcodec.lib libavutil/avutil.lib; do
    if [[ -f "$out/$library" ]]; then libraries+=("$out/$library"); fi
  done
  "$nm" --defined-only -g "${libraries[@]}" | awk 'NF == 3 {print $3}' >> "$work/defined.txt"
  "$nm" -u "${libraries[@]}" | awk 'NF == 2 {print $2}' >> "$work/used.txt"
}
build linux-x86_64 nm --cc=clang \
  --enable-ffnvcodec --enable-nvenc --enable-vaapi --enable-libvpl --enable-libdrm \
  --enable-hwaccel="$LINUX_HWACCELS" \
  --enable-encoder="$LINUX_ENCODERS" \
  --enable-decoder="$DECODERS" --enable-parser="$PARSERS"
# configure takes `lib` for Microsoft's when its usage says so, and finds
# the MSVC runtime, the SDK and nv-codec-headers through INCLUDE and LIB,
# which clang-cl and lld-link read, so no path of this machine lands in
# config.h.
mkdir -p "$work/bin"
for tool in clang-cl lld-link llvm-lib llvm-nm llvm-dlltool; do
  ln -s "$(find $llvm -path "*/bin/$tool" -print -quit)" "$work/bin/$tool"
done
printf '%s\n' '#!/usr/bin/env bash' \
  'if [ $# -eq 0 ]; then echo "Microsoft (R) Library Manager, as llvm-lib"; exit 0; fi' \
  'exec llvm-lib "$@"' > "$work/bin/lib"
chmod +x "$work/bin/lib"
(
  export PATH=$work/bin:$PATH
  # The import library vpl.dll's calls link through, from the functions
  # oneVPL's headers declare; configure's test is what links it.
  {
    echo 'LIBRARY vpl.dll'
    echo 'EXPORTS'
    grep -hoE 'MFX_CDECL[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\(' "$work"/vpl/api/vpl/*.h |
      sed -E 's/MFX_CDECL[[:space:]]+([A-Za-z_0-9]+).*/  \1/' | sort -u
  } > "$work/vpl.def"
  llvm-dlltool -m i386:x86-64 -d "$work/vpl.def" -l "$work/vpl-win/lib/vpl.lib"
  # nv-codec-headers' as well, which configure finds only through it.
  export PKG_CONFIG_PATH=$work/nvh/lib/pkgconfig:$work/vpl-pc-windows
  export INCLUDE="$work/nvh/include;$work/amf;$xwin/crt/include;$xwin/sdk/include/ucrt;$xwin/sdk/include/um;$xwin/sdk/include/shared"
  export LIB="$xwin/crt/lib/x86_64;$xwin/sdk/lib/um/x86_64;$xwin/sdk/lib/ucrt/x86_64"
  windows="clang-cl --target=x86_64-pc-windows-msvc"
  build windows-x86_64 llvm-nm --toolchain=msvc --cc="$windows" --cxx="$windows" --ld=lld-link \
    --ar=lib --nm=llvm-nm --target-os=win64 --arch=x86_64 --enable-cross-compile \
    --enable-ffnvcodec --enable-nvenc --enable-amf --enable-libvpl \
    --enable-encoder="$WINDOWS_ENCODERS" \
    --enable-d3d11va --enable-hwaccel="$WINDOWS_HWACCELS" \
    --enable-decoder="$DECODERS" --enable-parser="$PARSERS"
)

if grep -En '/(tmp|nix|home)/' "$work"/gen/*/config.{h,asm}; then
  echo "update.sh: configure's output names a path on this machine" >&2
  exit 1
fi

# The trimmed source: FFmpeg's files the builds read, and its licenses.
rm -rf "$here/ffmpeg" "$here/include" "$here/gen"
sed -n 's|^src/||p' "$work/read.txt" | sort -u | while read -r file; do
  mkdir -p "$here/ffmpeg/$(dirname "$file")"
  cp "$src/$file" "$here/ffmpeg/$file"
done
cp "$src"/{COPYING.LGPLv2.1,LICENSE.md,CREDITS,VERSION} "$here/ffmpeg/"
for change in "$here"/patches/*.patch; do
  patch -s -d "$here/ffmpeg" -p1 < "$change"
done
# Headers from outside FFmpeg: NVIDIA's loader, the AMF headers the Windows
# build read, libva's API, and oneVPL's, which both builds read.
mkdir -p "$here/include"
cp -r "$work/nvh/include/ffnvcodec" "$here/include/"
sed -n "s|^$work/amf/||p" "$work/read.txt" | sort -u | while read -r file; do
  mkdir -p "$here/include/$(dirname "$file")"
  cp "$work/amf/$file" "$here/include/$file"
done
cp -r "$libva/include/va" "$here/include/"
cp -r "$work/vpl/api/vpl" "$here/include/"
chmod -R u+w "$here/include"
# All are MIT, with the notice at the top of each header; packaging ships
# files named NOTICE.
{
  echo "include/ffnvcodec: nv-codec-headers $NVH_VERSION, https://github.com/FFmpeg/nv-codec-headers"
  sed -n '1,/\*\//p' "$here/include/ffnvcodec/nvEncodeAPI.h"
  echo
  echo "include/AMF: AMF $AMF_VERSION, https://github.com/GPUOpen-LibrariesAndSDKs/AMF"
  sed -n '1,/^\/\/ THE SOFTWARE\.$/p' "$here/include/AMF/core/Version.h"
  echo
  echo "include/va: libva $(sed -n 's/^#define VA_VERSION_S *"\(.*\)"/\1/p' "$here/include/va/va_version.h"), https://github.com/intel/libva"
  sed -n '1,/\*\//p' "$here/include/va/va.h"
  echo
  echo "include/vpl: oneVPL $VPL_VERSION, https://github.com/intel/libvpl"
  sed -n '1,/\*\//p' "$here/include/vpl/mfxdefs.h"
} > "$here/include/NOTICE"
cp -r "$work/gen" "$here/gen"

# FFmpeg's own symbols get a prefix, and so do the libva and oneVPL functions
# it calls, which src/va.c and src/vpl.c forward to those libraries once
# they're loaded.
# Not FFmpeg's: clang-cl's constants for Windows (string literals, vector
# and floating point constants), which the linker merges; the UCRT's stdio
# functions, which its headers define inline; and the GUIDs the Windows SDK's
# headers define for its interfaces, its displays and its video codecs, which
# Direct3D 11's decode names. Those are `DEFINE_GUID`'s `selectany` ones, which
# the linker merges, and every one of them is a name the SDK chose: `GUID_…`,
# `IID_…`, `DXVA_…` and the like, never FFmpeg's `av_`/`ff_` prefixes.
sort -u "$work/defined.txt" | grep -vxE 'av_log2(_16bit)?' |
  grep -vE '^(\?\?_C@|__(xmm|ymm|real)@|__local_stdio_)|^_?v?[sf]?n?w?(printf|scanf)(_s)?$|_GUID$|^(GUID|IID|CLSID|LIBID|FMTID|WKPDID|D3D|D3D11|D3D12|DXGI|DXVA|DXVA2)_' > "$work/defined.sorted"
mv "$work/defined.sorted" "$work/defined.txt"
# Renamed too, though named otherwise: the GUIDs the AMF headers define in
# each file that includes them, and a table hwcontext_amf.c leaves global.
grep -Ev '^(av|ff|avpriv|avcodec|avutil|avsubtitle)_|^(AMF[A-Za-z]*GUID|format_map)$' \
  "$work/defined.txt" && {
  echo "update.sh: a symbol without FFmpeg's prefixes" >&2
  exit 1
}
sort -u "$work/used.txt" | grep '^va' > "$work/va.txt" || true
sort -u "$work/used.txt" | grep '^MFX' > "$work/vpl.txt" || true
{
  echo "/* Generated by update.sh: renames FFmpeg's symbols, so they can't meet"
  echo " * the FFmpeg inside libwebrtc, and the libva and oneVPL functions"
  echo " * src/va.c and src/vpl.c forward. */"
  cat "$work/defined.txt" "$work/va.txt" "$work/vpl.txt" | awk '{print "#define " $1 " yf_" $1}'
} > "$here/gen/rename.h"
missing=$(comm -23 "$work/va.txt" <(grep -oE 'FORWARD\([^,]+, *va[A-Za-z0-9]+' "$here/src/va.c" |
  sed -E 's/.*, *//' | sort -u))
if [[ -n "$missing" ]]; then
  echo "update.sh: add these to src/va.c: $missing" >&2
  exit 1
fi
missing=$(comm -23 "$work/vpl.txt" \
  <(grep -oE '(FORWARD\([^,]+, *|FORWARD_VOID\()MFX[A-Za-z0-9_]+' "$here/src/vpl.c" |
    sed -E 's/.*[,(] *//' | sort -u))
if [[ -n "$missing" ]]; then
  echo "update.sh: add these to src/vpl.c: $missing" >&2
  exit 1
fi
echo "FFmpeg $FFMPEG_VERSION: $(find "$here/ffmpeg" -type f | wc -l) files," \
  "$(wc -l < "$here/gen/rename.h") renames"
