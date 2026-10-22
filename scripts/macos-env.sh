# Sourcing this replaces the Nix development shell for a macOS build.
#
#   source scripts/macos-env.sh
#   cargo build --locked --no-default-features --features desktop,discord --profile fast
#
# It points Cargo at the libwebrtc archive the Go Live codec shims and
# webrtc-sys both compile against. The shims read the headers and do not
# download them, so scripts/fetch-libwebrtc-mac unpacks the archive first.
# webrtc-sys-build locks its own download. That lock does not help a build
# script that only reads the directory.

YAPFAST_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"

# The same triple scripts/fetch-libwebrtc-mac unpacks. Release only: this
# tag has no debug archive, and LK_CUSTOM_WEBRTC overrides a debug triple.
case "$(uname -m)" in
  arm64) yapfast_triple=mac-arm64-release ;;
  x86_64) yapfast_triple=mac-x64-release ;;
  *) yapfast_triple=mac-arm64-release ;;
esac
export LK_CUSTOM_WEBRTC="$YAPFAST_ROOT/.local/libwebrtc-$yapfast_triple"
unset yapfast_triple

# Skia comes from rust-skia's binary cache, which skia-bindings downloads on
# the first build: the aarch64-apple-darwin archive of the commit 0.153.3 pins.
# Yapfast pins skia-safe with `no-compile`, so Skia is never built from source
# here. Set SKIA_BINARIES_URL only to point at a mirror of that archive.

# Clang from the Xcode command line tools builds libwebrtc's C++ bindings.
export CC=clang
export CXX=clang++

# The C++ standard library libwebrtc was built against is libc++, which macOS
# ships; nothing extra to add to LD_LIBRARY_PATH.
