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
        "camera_mf.cc",
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
    // the same module of the libwebrtc archive. A Windows build can be pointed at
    // native/camera_mf.cc instead, which films through Media Foundation and hands
    // a frame over as both the D3D11 texture it lies in and the I420 planes a
    // caller that reads memory wants; it names the same exports, so exactly one of
    // the two is built. Neither is verified on a Windows machine here.
    let media_foundation =
        target_os == "windows" && std::env::var("YAPFAST_CAMERA_MF").is_ok_and(|on| on != "0");
    println!("cargo:rerun-if-env-changed=YAPFAST_CAMERA_MF");
    if matches!(target_os.as_str(), "linux" | "windows") {
        let mut camera = cc::Build::new();
        camera
            .cpp(true)
            .file(if media_foundation {
                "native/camera_mf.cc"
            } else {
                "native/camera.cc"
            })
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
        if target_os == "windows" {
            if media_foundation {
                // Media Foundation's own runtime and the device manager it films
                // into a D3D11 texture through.
                for library in ["mfplat", "mf", "mfreadwrite", "mfuuid", "ole32", "d3d11"] {
                    println!("cargo:rustc-link-lib=dylib={library}");
                }
            } else {
                // DirectShow's device list reads each camera's name and path as a
                // VARIANT; webrtc-sys links the rest.
                println!("cargo:rustc-link-lib=dylib=oleaut32");
            }
        }
    }
    println!("cargo::rustc-check-cfg=cfg(hwenc)");
    println!("cargo::rustc-check-cfg=cfg(dmabufenc)");
    println!("cargo::rustc-check-cfg=cfg(hevcdec)");
    println!("cargo::rustc-check-cfg=cfg(vaapidec)");
    println!("cargo::rustc-check-cfg=cfg(vaapivpp)");
    println!("cargo::rustc-check-cfg=cfg(nvcuda)");
    println!("cargo::rustc-check-cfg=cfg(nvdec)");
    println!("cargo::rustc-check-cfg=cfg(mfxdec)");
    println!("cargo::rustc-check-cfg=cfg(d3d11enc)");
    println!("cargo::rustc-check-cfg=cfg(d3d11conv)");
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
        // The bridge between a capture and a session that reads surfaces:
        // native/d3d11convert.c runs the card's video processor over a BGRA
        // texture and writes NV12 at the stream's size, so Quick Sync and AMF
        // are handed a surface they can read instead of one they can't.
        let mut conv = cc::Build::new();
        conv.file("native/d3d11convert.c").warnings(true);
        use_clang_cl(&mut conv);
        if conv.get_compiler().is_like_msvc() {
            conv.flag("/std:c17");
        } else {
            conv.flag("-std:c17");
        }
        conv.compile("yapfast_d3d11conv");
        println!("cargo::rustc-cfg=d3d11conv");
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
                // The shim takes PipeWire's functions from the library it loads,
                // not from the link: see src/capture/pipewire.rs and native
                // pipewire_capture.c's `bind_api`, which is what keeps a weak
                // stub of them (libwebrtc's) from being the one called.
                for folder in &pipewire.folders {
                    println!("cargo:rustc-link-search=native={}", folder.display());
                }
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
        // The objects above call FFmpeg's and x264's own entry points. Those
        // libraries reach the binary through this crate's dependencies on the
        // vendored builds, but the shims' objects travel in a different crate's
        // rlib, so the addresses they call are named here too: one build script
        // answers for both the code and the libraries it borrows.
        println!("cargo:rustc-link-lib=static=yapfast_ffmpeg");
        println!("cargo:rustc-link-lib=static=x264");
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
    // The one place libcuda is started for the process, which the converter,
    // the session and the decoder all ask: `cuInit` doesn't survive two threads
    // calling it at once, which a test binary does when a decoder's open and an
    // encoder's run side by side. It has no dependencies of its own, so it is
    // built wherever any of them is.
    if target_os == "linux" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        let mut init = cc::Build::new();
        init.file("native/cuda_init.c").std("gnu17").warnings(true);
        init.compile("yapfast_cuda_init");
    }
    // NVIDIA's card as the way a screen's picture reaches its encoder on Linux:
    // native/cuda_convert.cu, the BGRA to NV12 scale and convert that runs on
    // the card, compiled here to the PTX the driver loads at run time.
    // native/cudaconvert.c then imports a captured DMA-BUF and launches it. No
    // CUDA toolkit is needed to build this or to run it — clang emits PTX, and
    // the driver that opens NVENC also loads it — so it is tried wherever a
    // Linux build with Go Live in it runs, and the rung is left out where it
    // can't be.
    if target_os == "linux" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        match cuda_kernel() {
            Ok(ptx) => {
                println!("cargo:rustc-env=YAPFAST_CUDA_PTX={}", ptx.display());
                // native/cudaconvert.c is the import and the launch: the
                // descriptor's memory as an EGLImage, that image as CUDA sees
                // it, and the kernels above run into an NV12 buffer on the card.
                // GNU C, because naming the device reads /proc/self/fd, which
                // strict C leaves undeclared.
                let mut convert = cc::Build::new();
                convert
                    .file("native/cudaconvert.c")
                    .std("gnu17")
                    .warnings(true);
                convert.compile("yapfast_cudaconvert");
                // native/cudaenc.c is the session on that buffer: NVENC opened on a
                // CUDA device, its input registered once, the tunings hwenc.c uses.
                // It sees FFmpeg's nvEncodeAPI header and loads the driver's own
                // encoder library, which is all an NVIDIA machine has.
                let mut encoder = cc::Build::new();
                encoder.file("native/cudaenc.c").std("gnu17").warnings(true);
                if let Some(folders) = std::env::var_os("DEP_FFMPEG_INCLUDE") {
                    encoder.includes(std::env::split_paths(&folders));
                }
                encoder.compile("yapfast_cudaenc");
                // Both libraries are dlopened, so a machine with neither still
                // starts: libcuda and libEGL are only wanted when a share asks
                // for the card, and `dl` is what asks.
                println!("cargo:rustc-link-lib=dylib=dl");
                println!("cargo::rustc-cfg=nvcuda");
            }
            Err(why) => println!("cargo:warning=no CUDA convert on the card ({why})"),
        }
    }
    let os = match target_os.as_str() {
        "windows" => "win",
        "macos" => "mac",
        _ => "linux",
    };
    // NVIDIA's card decoding a watched stream on Linux: native/hwdec_cuda.c asks
    // the driver's own NVDEC API directly — libnvcuvid and libcuda, both
    // dlopened, with the ffnvcodec headers vendored beside FFmpeg's — and exports
    // each decoded surface as a DMA-BUF for the tile. FFmpeg's own nvdec isn't in
    // this build, and its hwcontext_cuda could only hand a picture back through
    // system memory anyway, so this is the driver's API and not FFmpeg's. No CUDA
    // toolkit is needed to build it or to run it; a machine whose driver refuses
    // says so and the stream falls to the software decoder.
    if target_os == "linux" && std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        let mut nvdec = cc::Build::new();
        nvdec
            .file("native/hwdec_cuda.c")
            .std("gnu17")
            .warnings(true);
        nvdec.compile("yapfast_hwdec_cuda");
        println!("cargo::rustc-cfg=nvdec");
    }
    // Intel's own decoder, for the machine VA-API doesn't open: native/hwdec_mfx.c
    // decodes through oneVPL (MFX) on a VA display and exports the surface the
    // same way the VA-API shim does. oneVPL and libva are both dlopened, so a
    // machine with neither still starts and says why. It is built wherever this
    // target is and the vendored oneVPL and libva headers are there, which is
    // every Linux build of the Discord flavour.
    if target_os == "linux"
        && std::env::var_os("CARGO_FEATURE_DISCORD").is_some()
        && let Some(folders) = std::env::var_os("DEP_FFMPEG_INCLUDE")
    {
        let mut mfx = cc::Build::new();
        mfx.file("native/hwdec_mfx.c")
            .std("gnu17")
            .warnings(true)
            .include("native")
            .includes(std::env::split_paths(&folders));
        mfx.compile("yapfast_hwdec_mfx");
        println!("cargo::rustc-cfg=mfxdec");
    }
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

// native/cuda_convert.cu as PTX, in `OUT_DIR`: one file, two kernels, and no
// CUDA toolkit anywhere in the build. clang compiles CUDA to PTX on its own, and
// the types and builtins the kernel uses come from clang's own resource
// directory (`__clang_cuda_builtin_vars.h`, which is what says `threadIdx`)
// rather than from a toolkit's headers — that is what `-nocudainc` and the two
// defines here say.
fn cuda_kernel() -> Result<std::path::PathBuf, String> {
    for file in [
        "native/cuda_convert.cu",
        "native/cudaconvert.c",
        "native/cudaenc.c",
        "native/cudaconvert.h",
        "native/hwdec_cuda.c",
        "native/hwdec_cuda.h",
        "native/hwdec_mfx.c",
        "native/hwdec_mfx.h",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    // clang is the only compiler that emits PTX, and it is not always the one
    // this shell builds C with: `CC` may be gcc, which compiles the rest of
    // native/ and can do nothing here.
    let mut candidates = vec!["clang".to_owned()];
    for key in ["YAPFAST_CUDA_CLANG", "CC", "CXX"] {
        if let Ok(value) = std::env::var(key)
            && !candidates.contains(&value)
        {
            candidates.push(value);
        }
    }
    let mut why = vec![];
    for clang in candidates {
        match cuda_ptx(&clang) {
            Ok(out) => return Ok(out),
            Err(reason) => why.push(format!("{clang}: {reason}")),
        }
    }
    Err(why.join("; "))
}

/// `native/cuda_convert.cu` as PTX, with `clang`.
fn cuda_ptx(clang: &str) -> Result<std::path::PathBuf, String> {
    let resource = std::process::Command::new(clang)
        .arg("-print-resource-dir")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|folder| folder.trim().to_owned())
        .ok_or_else(|| format!("{clang} has no resource directory"))?;
    let out =
        std::path::PathBuf::from(std::env::var("OUT_DIR").map_err(|_| "no OUT_DIR".to_owned())?)
            .join("cuda_convert.ptx");
    // The oldest target whose PTX every newer card runs, and that anyone with a
    // driver new enough to encode still has: Maxwell, where R520 still runs and
    // NVENC has H.264 and HEVC. PTX runs on the arch it names and every newer
    // one, so one target covers the lot, and the older the target the more
    // drivers can load it — clang emits ISA 4.2 for this and ISA 6.3 for Turing,
    // and a modern driver takes both. What a card can encode is its own business:
    // AV1 needs Ada, HEVC Maxwell, H.264 Kepler, and an older card than this
    // falls through to the processor's copy like any other refusal.
    let arch = "sm_52";
    let result = std::process::Command::new(clang)
        .args([
            "-x",
            "cuda",
            "--cuda-path=/nonexistent",
            // One argument, not `--cuda-gpu-arch sm_75`: clang takes this one
            // joined, and refuses the split form.
            &format!("--cuda-gpu-arch={arch}"),
            "--cuda-device-only",
            "-nocudainc",
            "-nocudalib",
            "-O3",
            "-S",
            "-include",
            &format!("{resource}/include/__clang_cuda_builtin_vars.h"),
            "-D__device__=__attribute__((device))",
            "-D__global__=__attribute__((global))",
            "-o",
            out.to_str().ok_or("OUT_DIR is not UTF-8")?,
            "native/cuda_convert.cu",
        ])
        // Nix's clang wrapper adds `-fzero-call-used-regs`, which the PTX target
        // refuses: it is a hardening flag for native code, and this output is the
        // driver's to run.
        .env("NIX_HARDENING_ENABLE", "")
        .output()
        .map_err(|error| format!("clang didn't run: {error}"))?;
    if !result.status.success() {
        // What clang said, so a machine whose compiler can't do this says why
        // rather than only that it didn't.
        let said = String::from_utf8_lossy(&result.stderr);
        let said = said.trim();
        return Err(format!(
            "clang couldn't compile native/cuda_convert.cu to {arch} PTX ({})",
            said.lines().next().unwrap_or("no reason given")
        ));
    }
    Ok(out)
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
