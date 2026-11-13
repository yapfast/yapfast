//! Which of the card's own paths this build carries.
//!
//! The shims those paths call are C, and they stay in `yapfast-core`'s
//! `native/` for now, so this build script builds nothing: it only asks the
//! same questions `yapfast-core`'s build script asks when it decides whether to
//! build them, and sets the same cfgs, so that this crate's Rust and that
//! crate's C are gated the same way. When `native/` moves below this crate the
//! two scripts become one; until then the duplication is what keeps a build
//! from compiling a path whose shim isn't there, or leaving one out whose is.

use std::{env, path::PathBuf, process::Command};

fn main() {
    for cfg in [
        "hwenc",
        "dmabufenc",
        "hevcdec",
        "vaapidec",
        "vaapivpp",
        "nvcuda",
        "nvdec",
        "mfxdec",
        "d3d11enc",
        "d3d11conv",
        "d3d11dec",
        "wgc",
        "dup",
        "screencast",
    ] {
        println!("cargo::rustc-check-cfg=cfg({cfg})");
    }
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let discord = env::var_os("CARGO_FEATURE_DISCORD").is_some();
    // Which FFmpeg this build links, as ffmpeg-src's own build script answered:
    // a target it configured nothing for has no sources to build, and the
    // shims that read its headers — hwenc.c, hevc.c and the VA-API pair — are
    // built exactly where it did.
    let ffmpeg_built = env::var("DEP_FFMPEG_ENCODERS").as_deref() == Ok("1");
    println!("cargo::warning=yapfast-media: ffmpeg encoders {ffmpeg_built}");
    println!("cargo::rerun-if-changed=build.rs");
    if target_os == "windows" {
        // Desktop Duplication is DXGI, D3D11 and D3DCompiler alone, which every
        // Windows SDK that can build Yapfast has.
        println!("cargo::rustc-cfg=dup");
        // The bridge a capture hands a hardware encoder through, built here for
        // every Windows build; and NVENC's own D3D11 path beside it.
        println!("cargo::rustc-cfg=d3d11conv");
        println!("cargo::rustc-cfg=d3d11enc");
        if discord {
            // FFmpeg's D3D11 decoder, where a stream is watched at all.
            println!("cargo::rustc-cfg=d3d11dec");
            // Graphics Capture needs a Windows SDK whose WinRT headers carry
            // the API, which is the one thing not every SDK has.
            if graphics_capture_headers() {
                println!("cargo::rustc-cfg=wgc");
            }
        }
    }
    if target_os == "linux" && env::var_os("CARGO_FEATURE_SCREENCAST").is_some() && pipewire() {
        // The portal answers and the buffers it names are DMA-BUFs, read
        // without a copy; only a machine with PipeWire's headers has the shim.
        println!("cargo::rustc-cfg=screencast");
    }
    // The encoder plan this crate holds is Rust alone: it asks the adapters
    // which FFmpeg encoder each one can open, and reads the answer back from a
    // report. Its C side — native/hwenc.c — is `yapfast-core`'s to build, and
    // core sets its own `hwenc` where FFmpeg was configured with encoders. So
    // Windows takes the plan whenever a stream does, and Linux asks FFmpeg's
    // build script first, where the VA-API paths borrow its headers.
    if discord && (ffmpeg_built || target_os == "windows") {
        println!("cargo::rustc-cfg=hwenc");
    }
    if discord && ffmpeg_built {
        println!("cargo::rustc-cfg=hevcdec");
    }
    if target_os == "linux" && ffmpeg_built {
        let Some(folders) = env::var_os("DEP_FFMPEG_INCLUDE") else {
            return;
        };
        println!(
            "cargo::warning=yapfast-media: vaapi decode {}, import {}, vpp {}",
            vaapi_decode(&folders),
            vaapi_import(&folders),
            vaapi_vpp(&folders)
        );
        if vaapi_decode(&folders) {
            println!("cargo::rustc-cfg=vaapidec");
        }
        if vaapi_import(&folders) {
            println!("cargo::rustc-cfg=dmabufenc");
        }
        if vaapi_vpp(&folders) {
            println!("cargo::rustc-cfg=vaapivpp");
        }
    }
    if target_os == "linux" && discord {
        // NVIDIA's card converting a captured screen on the way to its
        // encoder: no toolkit is needed, only a clang that emits the PTX the
        // driver loads, which is the same compiler `yapfast-core` builds it
        // with.
        if clang() {
            println!("cargo::rustc-cfg=nvcuda");
        }
        // The driver's own NVDEC and oneVPL's decoder, both dlopened, so a
        // machine with neither still starts.
        println!("cargo::rustc-cfg=nvdec");
        if env::var_os("DEP_FFMPEG_INCLUDE").is_some() {
            println!("cargo::rustc-cfg=mfxdec");
        }
    }
}

/// Whether a clang is here, which is the only compiler that emits PTX and so
/// the only thing standing between this build and the card's own converter.
fn clang() -> bool {
    let mut candidates = vec!["clang".to_owned()];
    for key in ["YAPFAST_CUDA_CLANG", "CC", "CXX"] {
        println!("cargo::rerun-if-env-changed={key}");
        if let Ok(value) = env::var(key)
            && !candidates.contains(&value)
        {
            candidates.push(value);
        }
    }
    candidates
        .iter()
        .any(|clang| Command::new(clang).arg("--version").output().is_ok())
}

/// Whether the Windows SDK in this environment has the WinRT headers the
/// Graphics Capture shim is compiled against.
fn graphics_capture_headers() -> bool {
    let mut folders: Vec<PathBuf> = Vec::new();
    for path in env::split_paths(&env::var_os("INCLUDE").unwrap_or_default()) {
        folders.push(path.join("winrt"));
        folders.push(path);
    }
    for variable in ["WindowsSdkDir", "ProgramFiles(x86)", "ProgramFiles"] {
        println!("cargo::rerun-if-env-changed={variable}");
        let Some(root) = env::var_os(variable).map(PathBuf::from) else {
            continue;
        };
        let include = match variable {
            "WindowsSdkDir" => root.join("Include"),
            _ => root.join("Windows Kits").join("10").join("Include"),
        };
        // One directory per installed kit, each with its own winrt folder.
        let Ok(kits) = std::fs::read_dir(&include) else {
            continue;
        };
        for kit in kits.flatten() {
            folders.push(kit.path().join("winrt"));
        }
    }
    folders
        .iter()
        .any(|folder| folder.join("windows.graphics.capture.h").is_file())
}

/// Whether PipeWire's development files are here, which is what the portal's
/// shim is compiled against: `libpipewire`'s `pkg-config` file naming an
/// include directory.
fn pipewire() -> bool {
    let Ok(output) = Command::new("pkg-config")
        .args(["--cflags", "libpipewire-0.3"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .any(|word| word.starts_with("-I"))
}

/// The `config.h` and `config_components.h` FFmpeg was configured from, which
/// is what says which of its decoders and filters the library has.
fn ffmpeg_configured(folders: &std::ffi::OsStr) -> (Option<String>, String) {
    for folder in env::split_paths(folders) {
        let config = std::fs::read_to_string(folder.join("config.h")).ok();
        let components = std::fs::read_to_string(folder.join("config_components.h")).ok();
        if config.is_some() {
            return (config, components.unwrap_or_default());
        }
    }
    (None, String::new())
}

/// Whether a decoded picture can be exported as a DMA-BUF: VA-API, the DRM
/// PRIME export, and an H264 decoder to try it on.
fn vaapi_decode(folders: &std::ffi::OsStr) -> bool {
    let (Some(config), components) = ffmpeg_configured(folders) else {
        return false;
    };
    config.contains("#define CONFIG_VAAPI 1")
        && config.contains("#define CONFIG_LIBDRM 1")
        && components.contains("#define CONFIG_H264_VAAPI_HWACCEL 1")
}

/// Whether a DMA-BUF can be imported as a surface an encoder reads: the same
/// two, minus the decoder.
fn vaapi_import(folders: &std::ffi::OsStr) -> bool {
    let (Some(config), _) = ffmpeg_configured(folders) else {
        return false;
    };
    config.contains("#define CONFIG_VAAPI 1") && config.contains("#define CONFIG_LIBDRM 1")
}

/// Whether the card's video processor is available to convert a picture the
/// encoder can't read where it lies.
fn vaapi_vpp(folders: &std::ffi::OsStr) -> bool {
    let (Some(config), components) = ffmpeg_configured(folders) else {
        return false;
    };
    config.contains("#define CONFIG_AVFILTER 1")
        && components.contains("#define CONFIG_SCALE_VAAPI_FILTER 1")
}
