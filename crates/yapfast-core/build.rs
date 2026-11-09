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
        "hwenc_dmabuf.c",
        "hevc.c",
        "d3d11enc.c",
        "d3d11dec.cc",
        "hwdec.c",
        "hwdec.h",
        "hwdec_vaapi.c",
        "hwdec_vaapi.h",
        "wgc.cc",
        "dup.cc",
        "pipewire_capture.c",
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
    println!("cargo::rustc-check-cfg=cfg(dmabufenc)");
    println!("cargo::rustc-check-cfg=cfg(hevcdec)");
    println!("cargo::rustc-check-cfg=cfg(vaapidec)");
    println!("cargo::rustc-check-cfg=cfg(vaapivpp)");
    println!("cargo::rustc-check-cfg=cfg(d3d11enc)");
    println!("cargo::rustc-check-cfg=cfg(d3d11dec)");
    println!("cargo::rustc-check-cfg=cfg(wgc)");
    println!("cargo::rustc-check-cfg=cfg(dup)");
    println!("cargo::rustc-check-cfg=cfg(screencast)");
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
    // Linux's own end of the same idea: the desktop's screen sharing portal names
    // a PipeWire node whose frames are DMA-BUFs, and native/pipewire_capture.c
    // hands one buffer at a time to an encoder that reads it on the card; see
    // src/capture/pipewire.rs. Built where the `screencast` feature asks for it
    // and PipeWire's headers and library are found. A machine without them builds
    // the libwebrtc capturer and nothing here, which is what this crate did
    // before: a probe that finds nothing costs nothing, and a build that fails on
    // a machine without the headers costs everything. It sits above the return
    // below so that a build without Discord's Go Live still has it, since the
    // screen is shared by more than a stream.
    if target_os == "linux" && std::env::var_os("CARGO_FEATURE_SCREENCAST").is_some() {
        match pipewire() {
            Some(pipewire) => {
                let mut capture = cc::Build::new();
                capture
                    .file("native/pipewire_capture.c")
                    // gnu17 and not c17: PipeWire's headers read POSIX's
                    // `locale_t` and `ssize_t`, which a strict ISO C translation
                    // unit has no declaration for. `locale.h` and `sys/types.h`
                    // are put in front of the file for the same reason: it reads
                    // neither, and PipeWire's `spa/utils/string.h` uses both
                    // without including either.
                    .std("gnu17")
                    .flag("-include")
                    .flag("locale.h")
                    .flag("-include")
                    .flag("sys/types.h")
                    .warnings(true);
                for include in &pipewire.includes {
                    capture.include(include);
                }
                for flag in &pipewire.cflags {
                    capture.flag(flag);
                }
                capture.compile("yapfast_pipewire_capture");
                for folder in &pipewire.folders {
                    println!("cargo:rustc-link-search=native={}", folder.display());
                }
                println!("cargo:rustc-link-lib=dylib=pipewire-0.3");
                println!("cargo::rustc-cfg=screencast");
            }
            None => println!(
                "cargo:warning=PipeWire's development files were not found, so screen capture on the GPU isn't built: install pipewire's headers and libpipewire-0.3"
            ),
        }
    }
    // The other end of a watched stream: FFmpeg's Direct3D 11 decoder leaves
    // each picture in a D3D11 texture, and a video processor pass scales it
    // into the tile on the card; see native/d3d11dec.cc. Built with Discord's
    // Go Live, where a stream is watched at all, and only on Windows, whose
    // Direct3D 11 is what decodes it.
    if target_os == "windows" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        // FFmpeg's own Direct3D 11 decoder, where ffmpeg-src was built for this
        // target at all — which is what its `encoders` metadata says, since a
        // target it configured nothing for has no sources to build: its
        // native/hwdec.c decodes into a texture on the device the pass below
        // reads, and d3d11dec.cc calls it. A build without ffmpeg-src's
        // sources still compiles d3d11dec.cc, whose constructor then answers
        // that this build has no card decoder.
        let ffmpeg_built = std::env::var("DEP_FFMPEG_ENCODERS").as_deref() == Ok("1");
        let mut d3d11dec = cc::Build::new();
        d3d11dec
            .cpp(true)
            .file("native/d3d11dec.cc")
            .std("c++17")
            .warnings(true);
        if ffmpeg_built {
            d3d11dec.define("YAPFAST_FFDEC", "1");
        }
        // Windows-only sources carry their own guard, so a build elsewhere sees
        // an empty unit instead of headers no other compiler has.
        d3d11dec.define("YAPFAST_WINDOWS", "1");
        use_clang_cl(&mut d3d11dec);
        d3d11dec.compile("yapfast_d3d11dec");
        if ffmpeg_built {
            let mut hwdec = cc::Build::new();
            hwdec.file("native/hwdec.c").warnings(true);
            hwdec.define("YAPFAST_WINDOWS", "1");
            let folders = std::env::var_os("DEP_FFMPEG_INCLUDE").expect("ffmpeg-src's headers");
            hwdec.includes(std::env::split_paths(&folders));
            let rename = std::env::var("DEP_FFMPEG_RENAME").expect("ffmpeg-src's renames");
            use_clang_cl(&mut hwdec);
            // clang-cl, for Windows, spells the flags as MSVC does.
            if hwdec.get_compiler().is_like_msvc() {
                hwdec.flag("/std:c17").flag(format!("/FI{rename}"));
            } else {
                hwdec.flag("-std=c17").flag("-include").flag(&rename);
            }
            hwdec.compile("yapfast_hwdec");
        }
        // ole32, for the apartment and the strings the capture paths' COM
        // interface lists hand back. Nothing here starts an apartment of its
        // own: FFmpeg's decoder takes the device it is given.
        println!("cargo:rustc-link-lib=dylib=ole32");
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
    // The other end of a watched stream on Linux, where Direct3D 11's is on
    // Windows: FFmpeg's VA-API decoder leaves each picture in a surface on the
    // card and exports it as a DMA-BUF, which the tile pass imports; see
    // native/hwdec_vaapi.c. Built wherever this FFmpeg is, so that a machine
    // whose driver won't open a session is told so by the decoder's own open
    // and the stream falls to the software decoder. The crate decodes on the
    // card only where the FFmpeg it links was configured for it — its VA-API
    // decoders, and the DRM PRIME export a picture is handed over as — which
    // `vaapidec` says, so a build without them doesn't compile a path that
    // could only fail.
    if target_os == "linux" && std::env::var("DEP_FFMPEG_ENCODERS").as_deref() == Ok("1") {
        let folders = std::env::var_os("DEP_FFMPEG_INCLUDE").expect("ffmpeg-src's headers");
        let mut vaapi = cc::Build::new();
        vaapi
            .file("native/hwdec_vaapi.c")
            .file("native/hwenc_dmabuf.c")
            .includes(std::env::split_paths(&folders))
            .warnings(true);
        let rename = std::env::var("DEP_FFMPEG_RENAME").expect("ffmpeg-src's renames");
        // The card's video processor is FFmpeg's filter library, which a build
        // configured without it doesn't compile this file against: asked before
        // the compiler so that a build without it gets the encoder alone, which
        // is what this file was before the conversion existed.
        let vpp = vaapi_vpp(&folders);
        if vpp.is_ok() {
            vaapi.define("YAPFAST_VPP", None);
        }
        vaapi.flag("-std=c17").flag("-include").flag(&rename);
        vaapi.compile("yapfast_hwdec_vaapi");
        match vaapi_decode(&folders) {
            Ok(()) => println!("cargo:rustc-cfg=vaapidec"),
            Err(missing) => println!(
                "cargo:warning=this FFmpeg wasn't configured to decode on the card through VA-API ({missing}), so watched streams decode in software"
            ),
        }
        // The libdrm and VA-API device a picture is exported with are what one
        // is imported with, so a build that can hand a DMA-BUF out can hand one
        // in: native/hwenc_dmabuf.c reads the descriptor's buffer as a VA-API
        // surface with av_hwframe_map and the encoder reads that surface, with
        // no pixel of the picture copied.
        match vaapi_import(&folders) {
            Ok(()) => println!("cargo::rustc-cfg=dmabufenc"),
            Err(missing) => println!(
                "cargo:warning=this FFmpeg wasn't configured to encode a picture the card holds through VA-API ({missing}), so a DMA-BUF is copied back to the processor before it is encoded"
            ),
        }
        match vpp {
            Ok(()) => println!("cargo::rustc-cfg=vaapivpp"),
            Err(missing) => println!(
                "cargo:warning=this FFmpeg has no VA-API video processor ({missing}), so a picture the encoder can't read where it lies is converted on the processor"
            ),
        }
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

// Whether the FFmpeg this crate links was configured to decode on the card
// through VA-API: its VA-API decoders, which ffmpeg-src's configure enables
// with a `--enable-hwaccel=` per codec, and the DRM PRIME export
// av_hwframe_map needs to hand a picture over as a DMA-BUF, which is
// `--enable-libdrm` in the same configure. Ok when both are there, and the
// reason naming what isn't when either is not.
fn vaapi_decode(folders: &std::ffi::OsStr) -> Result<(), String> {
    let (mut config, mut components) = (None, None);
    for folder in std::env::split_paths(folders) {
        config = config.or_else(|| std::fs::read_to_string(folder.join("config.h")).ok());
        components =
            components.or_else(|| std::fs::read_to_string(folder.join("config_components.h")).ok());
    }
    // configure's output is what the library was built from, so a build
    // without it has no decoders to ask about either.
    let Some(config) = config else {
        return Err("configure's output isn't there".to_owned());
    };
    let mut missing = vec![];
    if !config.contains("#define CONFIG_VAAPI 1") {
        missing.push("no --enable-vaapi");
    }
    // Without libdrm, hwcontext_vaapi leaves the export of a surface as a
    // DMA-BUF out of the build, and av_hwframe_map answers that it can't.
    if !config.contains("#define CONFIG_LIBDRM 1") {
        missing.push("no --enable-libdrm for the DRM PRIME export");
    }
    let components = components.unwrap_or_default();
    if !components.contains("#define CONFIG_H264_VAAPI_HWACCEL 1") {
        missing.push("no --enable-hwaccel=h264_vaapi");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join(", "))
    }
}

// Whether the FFmpeg this crate links was configured to encode a picture the
// card already holds: its VA-API device (`--enable-vaapi`, which is also the
// encoder's) and the DRM PRIME import `av_hwframe_map` makes a surface with
// (`--enable-libdrm`). Ok when both are there, and the reason naming what isn't
// when either is not.
fn vaapi_import(folders: &std::ffi::OsStr) -> Result<(), String> {
    let mut config = None;
    for folder in std::env::split_paths(folders) {
        config = config.or_else(|| std::fs::read_to_string(folder.join("config.h")).ok());
    }
    let Some(config) = config else {
        return Err("configure's output isn't there".to_owned());
    };
    let mut missing = vec![];
    if !config.contains("#define CONFIG_VAAPI 1") {
        missing.push("no --enable-vaapi");
    }
    // Without libdrm, hwcontext_vaapi leaves importing a DMA-BUF as a surface
    // out of the build, and av_hwframe_map answers that it can't.
    if !config.contains("#define CONFIG_LIBDRM 1") {
        missing.push("no --enable-libdrm for the DRM PRIME import");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join(", "))
    }
}

// Whether the FFmpeg this crate links carries the card's video processor: its
// filter library (`--enable-avfilter`) and `scale_vaapi` in it, which is what
// turns a picture the encoder can't read as it arrived into the NV12 surface it
// reads, on the card. Ok when both are there, and the reason naming what isn't
// when either is not. A build configured with them but without VA-API has
// nothing to convert on, which `vaapi_import` refuses before this is asked.
fn vaapi_vpp(folders: &std::ffi::OsStr) -> Result<(), String> {
    let (mut config, mut components) = (None, None);
    for folder in std::env::split_paths(folders) {
        config = config.or_else(|| std::fs::read_to_string(folder.join("config.h")).ok());
        components =
            components.or_else(|| std::fs::read_to_string(folder.join("config_components.h")).ok());
    }
    let Some(config) = config else {
        return Err("configure's output isn't there".to_owned());
    };
    let mut missing = vec![];
    if !config.contains("#define CONFIG_AVFILTER 1") {
        missing.push("no --enable-avfilter");
    }
    if !components
        .unwrap_or_default()
        .contains("#define CONFIG_SCALE_VAAPI_FILTER 1")
    {
        missing.push("no --enable-filter=scale_vaapi");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join(", "))
    }
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

/// Where PipeWire's headers and library are, as its own `pkg-config` file names
/// them.
struct Pipewire {
    includes: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    cflags: Vec<String>,
}

/// Asks pkg-config for libpipewire, which is what says where its headers and its
/// library are. `None` when there is no pkg-config, no such package, or a
/// pkg-config whose answer names no include directory — every one of which is a
/// build that has no PipeWire to compile against.
fn pipewire() -> Option<Pipewire> {
    let output = Command::new("pkg-config")
        .args(["--cflags", "--libs", "libpipewire-0.3"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let answer = String::from_utf8_lossy(&output.stdout);
    let mut found = Pipewire {
        includes: Vec::new(),
        folders: Vec::new(),
        cflags: Vec::new(),
    };
    for word in answer.split_whitespace() {
        if let Some(folder) = word.strip_prefix("-I") {
            found.includes.push(PathBuf::from(folder));
        } else if let Some(folder) = word.strip_prefix("-L") {
            found.folders.push(PathBuf::from(folder));
        } else if !word.starts_with("-l") {
            // `-D_REENTRANT` and the like; the library is linked by name below.
            found.cflags.push(word.to_owned());
        }
    }
    (!found.includes.is_empty()).then_some(found)
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
