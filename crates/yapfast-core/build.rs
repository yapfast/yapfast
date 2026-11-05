//! Builds the Go Live codec shims, the GIF thumbnail encoder, the video file
//! reader and the cameras against the libvpx, FFmpeg, dav1d, libyuv and
//! WebRTC headers in the same libwebrtc archive webrtc-sys links, so they
//! match its code, and the H264 encoder against x264 (vendor/x264-src).

use std::{env, path::PathBuf, process::Command};

fn main() {
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
        "d3d11enc.c",
        "d3d11dec.cc",
        "wgc.cc",
        "dup.cc",
    ] {
        println!("cargo:rerun-if-changed=native/{file}");
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
    println!("cargo::rustc-check-cfg=cfg(d3d11enc)");
    println!("cargo::rustc-check-cfg=cfg(d3d11dec)");
    println!("cargo::rustc-check-cfg=cfg(wgc)");
    println!("cargo::rustc-check-cfg=cfg(dup)");
    // Windows' own capture path can hand NVENC its frames as D3D11 textures,
    // so d3d11enc.c encodes from the GPU without a frame's pixels being
    // converted or uploaded; see native/d3d11enc.c. It needs no library of
    // ours, only the ffnvcodec header it includes itself, so it is built here
    // where every Windows build gets it, rather than behind the encoders
    // FFmpeg has.
    if target_os == "windows" {
        let mut d3d11 = cc::Build::new();
        d3d11.file("native/d3d11enc.c").warnings(true);
        use_clang_cl(&mut d3d11);
        if d3d11.get_compiler().is_like_msvc() {
            d3d11.flag("/std:c17");
        } else {
            d3d11.flag("-std:c17");
        }
        d3d11.compile("yapfast_d3d11enc");
        // The device and its swap chain, the adapter list, DXGI's ids and the
        // shader the stand-in capture source is compiled from.
        for library in ["d3d11", "dxgi", "dxguid", "d3dcompiler"] {
            println!("cargo:rustc-link-lib=dylib={library}");
        }
        println!("cargo:rustc-cfg=d3d11enc");
    }
    // Windows' own capture path, the other end of the same idea: native/wgc.cc
    // fills a D3D11 texture from the screen, and d3d11enc.c above encodes it
    // where it lies. Built only where it can be — this target, a build with
    // Discord's Go Live in it, and a Windows SDK whose WinRT headers carry the
    // capture API — since a probe that finds nothing costs nothing, and a build
    // that fails on a machine without the headers costs everything.
    if target_os == "windows" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        if graphics_capture_headers() {
            let mut wgc = cc::Build::new();
            // C++, because the WinRT ABI headers declare classes rather than C
            // vtables: see native/wgc.cc.
            wgc.cpp(true)
                .file("native/wgc.cc")
                .std("c++17")
                .warnings(true);
            use_clang_cl(&mut wgc);
            wgc.compile("yapfast_wgc");
            // The device and its frame pool, DXGI's ids and its output
            // enumeration, the WinRT activation entry points (windowsapp), the
            // monitor handle a caller enumerates and its name (user32), the
            // apartment, the strings and the freed ones (ole32), and gdi32,
            // which the capture's own compositor side links against.
            for library in [
                "d3d11",
                "dxgi",
                "dxguid",
                "d3dcompiler",
                "windowsapp",
                "user32",
                "ole32",
                "gdi32",
            ] {
                println!("cargo:rustc-link-lib=dylib={library}");
            }
            println!("cargo::rustc-cfg=wgc");
        } else {
            println!(
                "cargo:warning=this Windows SDK has no Windows.Graphics.Capture header, so screen capture into a texture isn't built"
            );
        }
    }
    // Desktop Duplication, WGC's sibling: DXGI hands over the compositor's own
    // surface, and the turn to upright, the HDR conversion and the pointer happen
    // in one GPU pass on this side; see native/dup.cc. Built for every Windows
    // target rather than only Go Live's, because it asks nothing of the SDK but
    // DXGI, D3D11 and D3DCompiler, which every SDK that can build this crate
    // already has.
    if target_os == "windows" {
        let mut dup = cc::Build::new();
        // C++, for the shader source and the pointer's rules: see native/dup.cc.
        dup.cpp(true)
            .file("native/dup.cc")
            .std("c++17")
            .warnings(true);
        use_clang_cl(&mut dup);
        dup.compile("yapfast_dup");
        // The device and its textures, DXGI's output and its duplication, the
        // shader compiler that builds the one pass the frames are drawn with, and
        // user32, which has the desktop configuration the SDR white level is read
        // out of on an HDR screen.
        for library in ["d3d11", "dxgi", "dxguid", "d3dcompiler", "user32"] {
            println!("cargo:rustc-link-lib=dylib={library}");
        }
        println!("cargo::rustc-cfg=dup");
    }
    // The other end of a watched stream: the machine's own H.264, HEVC and AV1
    // decoders hand back a D3D11 texture, and a video processor pass scales it
    // into the tile on the card; see native/d3d11dec.cc. Built with Discord's
    // Go Live, where a stream is watched at all, and only on Windows, whose
    // Media Foundation is what decodes it: native/d3d11dec.cc needs the
    // mfplat and mfuuid libraries beside the d3d11 ones above.
    if target_os == "windows" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        let mut d3d11dec = cc::Build::new();
        d3d11dec
            .cpp(true)
            .file("native/d3d11dec.cc")
            .std("c++17")
            .warnings(true);
        use_clang_cl(&mut d3d11dec);
        d3d11dec.compile("yapfast_d3d11dec");
        // Media Foundation's platform and its interface ids, and ole32 for the
        // apartment and the strings the interface list hands back.
        for library in ["mfplat", "mfuuid", "ole32"] {
            println!("cargo:rustc-link-lib=dylib={library}");
        }
        println!("cargo::rustc-cfg=d3d11dec");
    }
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

// Whether the Windows SDK's WinRT headers on this machine carry Windows Graphics
// Capture: the one fact that decides whether native/wgc.cc can be compiled here
// at all, asked before the compiler so that a machine without those headers gets
// a build without the capture path rather than a failure in a file it can't
// read.
//
// Looked for where the compiler will look: first the INCLUDE a Visual Studio
// prompt puts there, which is also what cc passes on, and then the kits the SDK
// installs itself under, since cargo is normally run without one.
fn graphics_capture_headers() -> bool {
    let mut folders: Vec<PathBuf> = Vec::new();
    for path in env::split_paths(&env::var_os("INCLUDE").unwrap_or_default()) {
        folders.push(path.join("winrt"));
        folders.push(path);
    }
    for variable in ["WindowsSdkDir", "ProgramFiles(x86)", "ProgramFiles"] {
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
