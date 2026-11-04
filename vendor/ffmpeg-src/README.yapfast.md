# FFmpeg for Yapfast

Go Live's hardware encoders: FFmpeg 9.0.2's NVENC encoders for NVIDIA, on
Linux and Windows, on Windows its AMF ones for AMD, and on Linux its VA-API
ones for AMD and Intel, for H264, HEVC and AV1; and its HEVC decoder, for
watching H265 streams, which libwebrtc's FFmpeg can't decode, with its
x86-64 assembly. Nothing else of FFmpeg is built: no other decoders,
formats or filters.
`crates/yapfast-core/native/hwenc.c` drives the encoders and
`native/hevc.c` the decoder.

`update.sh` makes everything here except `src/` and `patches/`. It downloads
FFmpeg's release, nv-codec-headers 12.0.16.1 and AMD's AMF headers 1.5.2
and checks their SHA-256, takes libva's headers from the development
shell's nixpkgs, configures and builds FFmpeg for each target, and keeps
what follows. Windows is built with clang-cl against the MSVC runtime and
Windows SDK in cargo-xwin's cache, which `scripts/build-windows` fills the
first time; configure finds them through `INCLUDE` and `LIB`, so
`config.h` names no folder of the machine that ran it. When nothing else is set,
`build.rs` picks `clang-cl` for MSVC targets, because `gen/windows-x86_64` was
configured with clang-cl.

- `ffmpeg/`: the FFmpeg files that build read, unchanged but for `patches/`,
  under FFmpeg's LGPL-2.1-or-later (`ffmpeg/COPYING.LGPLv2.1`).
- `include/`: nv-codec-headers' `ffnvcodec/`, which loads NVIDIA's driver
  at run time, the AMF headers the Windows build read, `AMF/`, and libva's
  `va/`, all MIT (`include/NOTICE`).
- `gen/<os>-<arch>/`: what configure generated, `config.asm` among it, the
  C files to compile and the assembly to assemble (`sources.txt`,
  `asm.txt`), and configure's defines, include folders, NASM flags and
  system libraries, which `build.rs` uses as configure's makefiles would.
- `gen/rename.h`: a `#define` giving each of FFmpeg's symbols, and the libva
  functions it calls, a `yf_` prefix. Not clang-cl's constants or the UCRT's
  inline stdio functions, which every Windows library defines alike.

`build.rs` compiles those files with `gen/rename.h` included first, and
assembles the assembly with NASM's `--gprefix yf_`, which gives the same
prefix to every symbol the assembly defines or uses. The prefix matters: libwebrtc's archive holds Chromium's own FFmpeg, built to
decode, and three quarters of the names would otherwise meet. `src/va.c`
defines the `yf_va...` functions, which load libva on first use, so the
binary needs neither libva nor NVIDIA's libraries to start, and an encoder
that can't load them fails to open.

The minimum NVIDIA driver follows the nv-codec-headers version: 12.0 needs
520 or newer. Newer headers would raise it. AMF's do the same for AMD's
driver: FFmpeg asks its AMF runtime for the headers' version, and 1.5.2 is
the oldest FFmpeg 9 builds with. FFmpeg loads `amfrt64.dll` from the system
folder only when an AMF encoder opens.

## Changes

- `nvenc-bitrate-without-idr.patch`: NVENC changes its bitrate as it runs,
  where FFmpeg resets the encoder, which costs an IDR frame each time. Go
  Live follows Discord's bandwidth estimate, which moves often.
- `vaapi-bitrate-without-idr.patch`: the same for VA-API. FFmpeg sends the
  rate control parameters to the driver only with IDR frames; this sends
  them again with the next picture when the bitrate changes, which VA-API
  allows with any picture.
- `amf-bitrate-without-idr.patch`: the same for AMF, which FFmpeg gives the
  bitrate only when it opens the encoder. AMF takes the target, peak and
  buffer size as it runs; the patch sets them before the next frame when
  the bitrate changes, the peak first when it rises.

## Updating

Change the versions and hashes at the top of `update.sh`, run it, and build:
it fails when FFmpeg starts calling a libva function `src/va.c` lacks, or
when a patch no longer applies.
