//! Builds the Go Live codec shims, the GIF thumbnail encoder, the video file
//! reader and the cameras against the libvpx, FFmpeg, dav1d, libyuv and
//! WebRTC headers in the same libwebrtc archive webrtc-sys links, so they
//! match its code, and the H264 encoder against x264 (vendor/x264-src).

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
    ] {
        println!("cargo:rerun-if-changed=native/{file}");
    }
    let include = webrtc_sys_build::webrtc_dir().join("include");
    let third_party = include.join("third_party");
    cc::Build::new()
        .file("native/picture.c")
        .include(third_party.join("libyuv/include"))
        .warnings(true)
        .compile("yapfast_picture");
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
    cc::Build::new()
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
        .warnings(true)
        .compile("yapfast_codecs");
}
