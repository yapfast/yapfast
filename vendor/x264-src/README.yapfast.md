# x264 for Yapfast

`x264/` is x264 at commit 373697b467f7cd0af88f1e9e32d4f10540df4687
(2025-01-03, core 164) from https://code.videolan.org/videolan/x264, under its
original GPL-2.0-or-later license (`x264/COPYING`). Its files are unchanged;
only those a library build for 8-bit 4:2:0 video uses are kept: `x264.h`,
`common/` with `common/x86/`, and `encoder/`, without OpenCL. The
command-line tool, its filters and the other processors' code are left out.

`build.rs` stands in for x264's configure and Makefile. It writes the
`config.h` and `x264_config.h` configure would for Linux and Windows,
compiles the C files with the compiler `cc` picks (clang, or clang-cl for
Windows), and assembles the x86-64 assembly with nasm (`$NASM`, or `nasm` on
the PATH), as ELF, Win64 or, for an Intel Mac, Mach-O. On other processors it builds the plain C code, without assembly.
Two settings differ from configure's: the stack alignment is 16 bytes on every
compiler, which x264's assembly realigns for, and transparent huge pages stay
off, which would round x264's large buffers up to 2 MiB pages.

To move to a newer x264, copy the same folders from its source, update the
file lists in `build.rs` from its Makefile (`make -n lib-static` after
`./configure --disable-cli --bit-depth=8 --chroma-format=420
--disable-opencl` shows them), and the commit in `X264_CONFIG` and here.
