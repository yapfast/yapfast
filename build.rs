//! Gives the Windows executable its icon: compiles packaging/windows/yapfast.rc
//! with LLVM's resource compiler and links the result. On Linux, lets a plain
//! `cargo build` link rust-skia's own Skia archive beside libwebrtc. On macOS
//! it compiles native/appkit.m, the menu bar, the tray icon and the
//! notifications.

use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows");
    println!("cargo:rerun-if-env-changed=SKIA_BINARIES_URL");
    println!("cargo:rerun-if-changed=native/appkit.m");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => windows_icon(),
        Ok("linux") => linux_skia(),
        Ok("macos") => macos(),
        _ => {}
    }
}

/// The Mac's own layer, and the link its WebRTC needs.
///
/// `native/appkit.m` is compiled with automatic reference counting, since it
/// keeps AppKit objects, and the frameworks it calls are named here.
///
/// `-ObjC` is the part that isn't obvious. libwebrtc reaches for NSString
/// categories compiled into it, such as `+[NSString stringForAbslStringView:]`,
/// and nothing names one, so the linker leaves out the archive member holding
/// them and asking for one crashes: opening Settings builds the voice device
/// list, which asks the VP9 encoder which scalabilities it supports.
/// webrtc-sys asks for `-ObjC` too, but Cargo only honours a link argument for
/// targets of the package that emits it, so the executable has to ask here.
fn macos() {
    cc::Build::new()
        .file("native/appkit.m")
        .flag("-fobjc-arc")
        .warnings(true)
        .compile("yapfast_appkit");
    println!("cargo:rustc-link-arg=-ObjC");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=CoreServices");
    println!("cargo:rustc-link-lib=framework=ServiceManagement");
    println!("cargo:rustc-link-lib=framework=UserNotifications");
}

fn windows_icon() {
    let resources = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("yapfast.res");
    // The script names only the icon beside it, so it needs no preprocessor.
    let status = Command::new("llvm-rc")
        .args(["-no-preprocess", "-FO"])
        .arg(&resources)
        .arg("packaging/windows/yapfast.rc")
        .status()
        .unwrap_or_else(|error| panic!("could not run llvm-rc: {error}"));
    assert!(status.success(), "llvm-rc could not compile yapfast.rc");
    // The MSVC linker takes compiled resources as an input.
    println!("cargo:rustc-link-arg-bins={}", resources.display());
}

/// libwebrtc's libjpeg-turbo prefixes its functions with `chromium_`, but not
/// its table `jpeg_natural_order`: Chromium lists only functions to prefix.
/// The libjpeg-turbo in rust-skia's Skia archive defines the same table, so
/// the two clash at the link. The Nix shells and the Arch package set
/// `SKIA_BINARIES_URL` to a copy of the archive with Skia's table renamed
/// (`nix/prepare-skia-native.py`) and link without this flag, so a new clash
/// still fails there. Without it, the linker keeps the first of two
/// definitions; both tables hold the same 320 bytes.
fn linux_skia() {
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none() || env::var_os("SKIA_BINARIES_URL").is_some()
    {
        return;
    }
    // For the tests' executables too, which link Skia as well.
    println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
}
