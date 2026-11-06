//! Builds x264's encoder library from x264/: 8-bit 4:2:0 video only, without
//! its command-line tool or OpenCL, as x264's configure and Makefile would
//! with `--disable-cli --bit-depth=8 --chroma-format=420 --disable-opencl`.
//! The configuration configure would detect is written here, for Linux and
//! Windows; x86-64 gets x264's assembly, which nasm assembles ($NASM, or
//! nasm on the PATH) for the target's object format, including an Intel Mac's,
//! and other processors its plain C.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Built once, whatever the bit depth.
const COMMON: &[&str] = &[
    "common/osdep.c",
    "common/base.c",
    "common/cpu.c",
    "common/tables.c",
    "encoder/api.c",
];
/// Built once for each bit depth; only 8 here.
const PER_DEPTH: &[&str] = &[
    "common/mc.c",
    "common/predict.c",
    "common/pixel.c",
    "common/macroblock.c",
    "common/frame.c",
    "common/dct.c",
    "common/cabac.c",
    "common/common.c",
    "common/rectangle.c",
    "common/set.c",
    "common/quant.c",
    "common/deblock.c",
    "common/vlc.c",
    "common/mvpred.c",
    "common/bitstream.c",
    "encoder/analyse.c",
    "encoder/me.c",
    "encoder/ratecontrol.c",
    "encoder/set.c",
    "encoder/macroblock.c",
    "encoder/cabac.c",
    "encoder/cavlc.c",
    "encoder/encoder.c",
    "encoder/lookahead.c",
    "common/threadpool.c",
];
/// The x86 C files, built for each bit depth.
const X86_C: &[&str] = &["common/x86/mc-c.c", "common/x86/predict-c.c"];
/// x86-64 assembly for each bit depth; common/x86/cpu-a.asm, which has
/// none, comes first.
const X86_ASM: &[&str] = &[
    "dct-64.asm",
    "trellis-64.asm",
    "bitstream-a.asm",
    "const-a.asm",
    "cabac-a.asm",
    "dct-a.asm",
    "deblock-a.asm",
    "mc-a.asm",
    "mc-a2.asm",
    "pixel-a.asm",
    "predict-a.asm",
    "quant-a.asm",
    "sad-a.asm",
];

/// What x264.h includes to know how the library was built.
const X264_CONFIG: &str = "#define X264_GPL           1
#define X264_INTERLACED    1
#define X264_BIT_DEPTH     8
#define X264_CHROMA_FORMAT X264_CSP_I420
#define X264_VERSION \" 373697b\"
#define X264_POINTVER \"0.164.x 373697b\"
";

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("x264");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let target = Target::from_env();
    println!("cargo:rerun-if-changed=x264");
    println!("cargo:rerun-if-env-changed=NASM");

    let include = out.join("include");
    fs::create_dir_all(&include).expect("an include folder");
    fs::write(include.join("config.h"), target.config_h()).expect("config.h");
    fs::write(include.join("x264_config.h"), X264_CONFIG).expect("x264_config.h");
    fs::copy(root.join("x264.h"), include.join("x264.h")).expect("x264.h");
    println!("cargo:include={}", include.display());

    let mut objects = build(&root, &include, &target, false).compile_intermediates();
    let mut per_depth = build(&root, &include, &target, true);
    if target.x86_64 {
        per_depth.files(X86_C.iter().map(|file| root.join(file)));
    }
    per_depth.files(PER_DEPTH.iter().map(|file| root.join(file)));
    objects.extend(per_depth.compile_intermediates());
    if target.x86_64 {
        objects.extend(assemble(&root, &out, &target));
    }
    cc::Build::new().objects(objects).compile("x264");
    if target.os == "linux" {
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
    }
}

/// The C files built once, or with `per_depth` the 8-bit ones' settings
/// (the files are added by the caller).
fn build(root: &Path, include: &Path, target: &Target, per_depth: bool) -> cc::Build {
    let mut build = cc::Build::new();
    build
        .include(include)
        .include(root)
        // An encoder is slow unoptimized, even in a debug build.
        .opt_level(3)
        .warnings(false)
        .define("NDEBUG", None);
    if per_depth {
        build.define("HIGH_BIT_DEPTH", "0").define("BIT_DEPTH", "8");
    } else {
        build.files(COMMON.iter().map(|file| root.join(file)));
        if target.os == "windows" {
            build.file(root.join("common/win32thread.c"));
        }
    }
    if target.msvc {
        build.flag_if_supported("-fp:fast");
    } else {
        build
            .flag("-std=gnu99")
            .flag("-ffast-math")
            // With __FAST_MATH__ glibc declares vector math, and GCC links
            // libmvec, which not every distribution has, for log2 loops that
            // run once per encoder.
            .flag("-U__FAST_MATH__")
            .flag("-fomit-frame-pointer")
            .flag("-fvisibility=hidden");
        if target.os == "linux" {
            build.define("_GNU_SOURCE", None);
        }
    }
    build
}

/// Assembles common/x86's assembly for x86-64, returning the objects.
fn assemble(root: &Path, out: &Path, target: &Target) -> Vec<PathBuf> {
    let nasm = env::var_os("NASM").unwrap_or_else(|| "nasm".into());
    let x86 = root.join("common/x86");
    let format = match target.os.as_str() {
        "windows" => "win64",
        "macos" => "macho64",
        _ => "elf64",
    };
    let files =
        std::iter::once(("cpu-a.asm", false)).chain(X86_ASM.iter().map(|file| (*file, true)));
    let mut objects = vec![];
    for (file, per_depth) in files {
        let object = out.join(format!("asm-{}.o", file.trim_end_matches(".asm")));
        let mut command = Command::new(&nasm);
        // nasm joins include paths to file names as they are.
        command
            .arg(format!("-I{}/", root.display()))
            .arg(format!("-I{}/", x86.display()))
            .args(["-DARCH_X86_64=1", "-DSTACK_ALIGNMENT=16", "-f", format]);
        if target.os != "windows" {
            command.arg("-DPIC");
        }
        // Mach-O symbols carry a leading underscore, which x264's assembly
        // adds with PREFIX, as its configure does for macOS.
        if target.os == "macos" {
            command.arg("-DPREFIX");
        }
        if per_depth {
            command.args(["-DBIT_DEPTH=8", "-Dprivate_prefix=x264_8"]);
        }
        command.arg("-o").arg(&object).arg(x86.join(file));
        match command.status() {
            Ok(status) if status.success() => objects.push(object),
            Ok(status) => panic!("nasm failed on {file}: {status}"),
            Err(error) => panic!(
                "could not run nasm ({error}), which assembles x264's x86-64 code: \
                 install it, or set NASM to its path"
            ),
        }
    }
    objects
}

/// What x264's configure would find out about the target.
struct Target {
    os: String,
    x86_64: bool,
    /// Microsoft's compiler or clang-cl, without GCC's extensions.
    msvc: bool,
}

impl Target {
    fn from_env() -> Self {
        let var = |name| env::var(name).unwrap_or_default();
        Self {
            os: var("CARGO_CFG_TARGET_OS"),
            x86_64: var("CARGO_CFG_TARGET_ARCH") == "x86_64",
            msvc: var("CARGO_CFG_TARGET_ENV") == "msvc",
        }
    }

    /// config.h as configure writes it. Transparent huge pages stay off:
    /// x264 would round its large buffers up to 2 MiB pages for them.
    fn config_h(&self) -> String {
        let linux = self.os == "linux";
        let windows = self.os == "windows";
        let gnu = !self.msvc;
        let mut defines = vec![
            ("HAVE_MALLOC_H", u8::from(linux)),
            ("HAVE_X86_INLINE_ASM", u8::from(self.x86_64 && gnu)),
            ("HAVE_MMX", u8::from(self.x86_64)),
            ("ARCH_X86_64", u8::from(self.x86_64)),
            ("SYS_LINUX", u8::from(linux)),
            ("SYS_WINDOWS", u8::from(windows)),
            ("HAVE_POSIXTHREAD", u8::from(!windows)),
            ("HAVE_WIN32THREAD", u8::from(windows)),
            ("HAVE_THREAD", 1),
            ("HAVE_CPU_COUNT", u8::from(linux)),
            ("HAVE_SYNC_FETCH_AND_ADD", u8::from(gnu)),
            ("HAVE_LOG2F", 1),
            ("HAVE_STRTOK_R", u8::from(!windows)),
            ("HAVE_CLOCK_GETTIME", u8::from(linux)),
            ("HAVE_GETAUXVAL", u8::from(linux)),
            ("HAVE_SYSCONF", u8::from(!windows)),
            ("HAVE_MMAP", u8::from(!windows)),
            ("HAVE_THP", 0),
            ("HAVE_VECTOREXT", u8::from(gnu)),
            ("HAVE_BITDEPTH8", 1),
            ("HAVE_BITDEPTH10", 0),
            ("HAVE_GPL", 1),
            ("HAVE_INTERLACED", 1),
        ];
        for name in [
            "HAVE_ALTIVEC",
            "HAVE_ALTIVEC_H",
            "HAVE_ARMV6",
            "HAVE_ARMV6T2",
            "HAVE_NEON",
            "HAVE_AARCH64",
            "HAVE_BEOSTHREAD",
            "HAVE_SWSCALE",
            "HAVE_LAVF",
            "HAVE_FFMS",
            "HAVE_GPAC",
            "HAVE_AVS",
            "HAVE_OPENCL",
            "HAVE_LSMASH",
            "HAVE_AS_FUNC",
            "HAVE_INTEL_DISPATCHER",
            "HAVE_MSA",
            "HAVE_LSX",
            "HAVE_WINRT",
            "HAVE_VSX",
            "HAVE_ARM_INLINE_ASM",
            "HAVE_SVE",
            "HAVE_SVE2",
            "HAVE_ELF_AUX_INFO",
        ] {
            defines.push((name, 0));
        }
        // configure defines only what holds, but leaves the HAVE_ ones it
        // tests at 0; the rest, such as ARCH_ and SYS_, it leaves out.
        let mut config = String::new();
        for (name, value) in defines {
            if value == 1 || name.starts_with("HAVE_") {
                config.push_str(&format!("#define {name} {value}\n"));
            }
        }
        config.push_str("#define STACK_ALIGNMENT 16\n");
        config.push_str("#define CHROMA_FORMAT CHROMA_420\n");
        if linux {
            config.push_str("#define fseek fseeko\n#define ftell ftello\n");
        }
        if self.msvc && self.x86_64 {
            config.push_str("#ifndef __SSE__\n#define __SSE__ 1\n#endif\n");
        }
        config
    }
}
