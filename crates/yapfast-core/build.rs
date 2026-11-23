//! Builds the Go Live codec shims, the GIF thumbnail encoder, the video file
//! reader and the cameras against the libvpx, FFmpeg, dav1d, libyuv and
//! WebRTC headers in the same libwebrtc archive webrtc-sys links, so they
//! match its code, and the H264 encoder against x264 (vendor/x264-src).

use std::fmt::Write as _;
use std::{env, path::PathBuf, process::Command};

fn main() {
    theme_template();
    for file in [
        "vp8.c",
        "h264.c",
        "clip.c",
        "decode.c",
        "av1.c",
        "video.c",
        "picture.c",
        "camera.cc",
        "hwenc.c",
        "hevc.c",
    ] {
        println!("cargo:rerun-if-changed=native/{file}");
    }
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");
    // This build script can run before webrtc-sys has downloaded its headers.
    // Use the helper's shared download lock rather than depending on build order.
    if webrtc_sys_build::custom_dir().is_none() {
        webrtc_sys_build::download_webrtc().expect("could not prepare WebRTC headers");
    }
    let include = webrtc_sys_build::webrtc_dir().join("include");
    let third_party = include.join("third_party");
    let mut picture = cc::Build::new();
    picture
        .file("native/picture.c")
        .include(third_party.join("libyuv/include"))
        .warnings(true);
    use_clang_cl(&mut picture);
    picture.compile("yapfast_picture");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // Linux captures through V4L2, and Windows through DirectShow, both in
    // the same module of the libwebrtc archive.
    if matches!(target_os.as_str(), "linux" | "windows") {
        let mut camera = cc::Build::new();
        camera
            .cpp(true)
            .file("native/camera.cc")
            .include(&include)
            .include(third_party.join("abseil-cpp"))
            .include(third_party.join("libyuv/include"))
            .std("c++20")
            .flag("-Wno-deprecated-declarations")
            // libwebrtc's headers mark some pointers and not others.
            .flag("-Wno-nullability-completeness")
            .warnings(true);
        use_clang_cl(&mut camera);
        // The toolchain sets its own hardening mode.
        for (key, value) in webrtc_sys_build::webrtc_defines() {
            if key != "_LIBCPP_HARDENING_MODE" {
                camera.define(&key, value.as_deref());
            }
        }
        camera.compile("yapfast_camera");
        // DirectShow's device list reads each camera's name and path as a
        // VARIANT; webrtc-sys links the rest.
        if target_os == "windows" {
            println!("cargo:rustc-link-lib=dylib=oleaut32");
        }
    }
    println!("cargo::rustc-check-cfg=cfg(hwenc)");
    println!("cargo::rustc-check-cfg=cfg(hevcdec)");
    if std::env::var_os("CARGO_FEATURE_DISCORD").is_none() {
        return;
    }
    // FFmpeg's hardware encoders and HEVC decoder, where ffmpeg-src builds
    // them: hwenc.c and hevc.c see only their headers, with FFmpeg's names
    // prefixed as in the library.
    if std::env::var("DEP_FFMPEG_ENCODERS").as_deref() == Ok("1") {
        let mut ffmpeg = cc::Build::new();
        use_clang_cl(&mut ffmpeg);
        ffmpeg
            .file("native/hwenc.c")
            .file("native/hevc.c")
            .warnings(true);
        let folders = std::env::var_os("DEP_FFMPEG_INCLUDE").expect("ffmpeg-src's headers");
        ffmpeg.includes(std::env::split_paths(&folders));
        let rename = std::env::var("DEP_FFMPEG_RENAME").expect("ffmpeg-src's renames");
        // clang-cl, for Windows, spells the flags as MSVC does.
        if ffmpeg.get_compiler().is_like_msvc() {
            ffmpeg.flag("/std:c17").flag(format!("/FI{rename}"));
        } else {
            ffmpeg.flag("-std=c17").flag("-include").flag(&rename);
        }
        ffmpeg.compile("yapfast_hwenc");
        println!("cargo:rustc-cfg=hwenc");
        println!("cargo:rustc-cfg=hevcdec");
    }
    let os = match target_os.as_str() {
        "windows" => "win",
        "macos" => "mac",
        _ => "linux",
    };
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        _ => "x64",
    };
    let mut codecs = cc::Build::new();
    codecs
        .file("native/vp8.c")
        .file("native/h264.c")
        .file("native/clip.c")
        .file("native/decode.c")
        .file("native/av1.c")
        .file("native/video.c")
        .include(third_party.join("libvpx/source/libvpx"))
        // x264-src's build script hands over its headers' folder.
        .include(std::env::var_os("DEP_X264_INCLUDE").expect("x264-src's headers"))
        .include(third_party.join("ffmpeg"))
        .include(third_party.join("dav1d/libdav1d/include"))
        .include(third_party.join("libyuv/include"))
        // FFmpeg's configuration for the Chrome build libwebrtc uses.
        .include(third_party.join(format!("ffmpeg/chromium/config/Chrome/{os}/{arch}")))
        .warnings(true);
    use_clang_cl(&mut codecs);
    codecs.compile("yapfast_codecs");
}

// camera.cc uses Clang-only warning flags, which cl.exe rejects. Switch only
// cc's own MSVC default to clang-cl; a person's compiler choice wins.
fn use_clang_cl(build: &mut cc::Build) {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") || has_explicit_compiler() {
        return;
    }
    if build.get_compiler().is_like_clang() {
        return;
    }
    let clang_cl = find_clang_cl().unwrap_or_else(|| {
        panic!(
            "this MSVC build needs clang-cl; install LLVM for Windows and add its bin directory to PATH, or install Visual Studio's C++ Clang tools for Windows component"
        )
    });
    build.compiler(clang_cl);
}

fn has_explicit_compiler() -> bool {
    if env::var_os("CC").is_some() || env::var_os("CXX").is_some() {
        return true;
    }
    let Ok(target) = env::var("TARGET") else {
        return false;
    };
    let underscored_target = target.replace('-', "_");
    ["CC", "CXX"].iter().any(|variable| {
        env::var_os(format!("{variable}_{target}")).is_some()
            || env::var_os(format!("{variable}_{underscored_target}")).is_some()
    })
}

fn find_clang_cl() -> Option<PathBuf> {
    env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path)
                .map(|folder| folder.join("clang-cl.exe"))
                .find(|path| path.is_file())
        })
        .or_else(|| {
            env::var_os("VCINSTALLDIR")
                .map(|vc| PathBuf::from(vc).join("Tools/Llvm/x64/bin/clang-cl.exe"))
                .filter(|path| path.is_file())
        })
        .or_else(|| {
            let vswhere = PathBuf::from(env::var_os("ProgramFiles(x86)")?)
                .join("Microsoft Visual Studio/Installer/vswhere.exe");
            let output = Command::new(vswhere)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-find",
                    r"**\Llvm\**\clang-cl.exe",
                ])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| PathBuf::from(line.trim()))
                .find(|path| path.is_file())
        })
        .or_else(|| {
            env::var_os("ProgramFiles")
                .map(|folder| PathBuf::from(folder).join("LLVM/bin/clang-cl.exe"))
                .filter(|path| path.is_file())
        })
}

/// Lists `themes/template/` for `yapfast theme new`, which writes a new
/// theme's folder from it: `OUT_DIR/theme_template.rs` holds each file's
/// path in the folder and its bytes.
fn theme_template() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../themes/template");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    let mut folders = vec![root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).expect("themes/template is there") {
            let path = entry.expect("themes/template reads").path();
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else {
                println!("cargo:rerun-if-changed={}", path.display());
                let relative = path
                    .strip_prefix(&root)
                    .unwrap()
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push((relative, path.canonicalize().unwrap()));
            }
        }
    }
    files.sort();
    let mut out =
        String::from("/// `themes/template/`'s files: each path in the folder and its bytes.\n");
    out.push_str("pub(crate) const TEMPLATE: &[(&str, &[u8])] = &[\n");
    for (relative, path) in &files {
        let _ = writeln!(
            out,
            "    ({relative:?}, include_bytes!({:?})),",
            path.display().to_string()
        );
    }
    out.push_str("];\n");
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join("theme_template.rs");
    std::fs::write(target, out).expect("OUT_DIR is writable");
}
