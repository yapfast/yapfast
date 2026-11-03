//! Gives the Windows executable its icon: compiles packaging/windows/yapfast.rc
//! with the available resource compiler and links the result. On Linux, lets a
//! plain `cargo build` link rust-skia's own Skia archive beside libwebrtc.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows");
    println!("cargo:rerun-if-env-changed=SKIA_BINARIES_URL");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => windows_icon(),
        Ok("linux") => linux_skia(),
        _ => {}
    }
}

fn windows_icon() {
    let resources = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("yapfast.res");
    // The script names only the icon beside it, so it needs no preprocessor.
    let status = match Command::new("llvm-rc")
        .current_dir("packaging/windows")
        .args(["-no-preprocess", "-FO"])
        .arg(&resources)
        .arg("yapfast.rc")
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let rc = find_windows_rc().unwrap_or_else(|| {
                panic!("could not find llvm-rc or the Windows SDK's rc.exe; install LLVM for Windows and add its bin directory to PATH")
            });
            Command::new(rc)
                .current_dir("packaging/windows")
                .args(["/fo"])
                .arg(&resources)
                .arg("yapfast.rc")
                .status()
                .unwrap_or_else(|error| panic!("could not run the Windows SDK's rc.exe: {error}"))
        }
        Err(error) => panic!("could not run llvm-rc: {error}"),
    };
    assert!(
        status.success(),
        "resource compiler could not compile yapfast.rc"
    );
    // The MSVC linker takes compiled resources as an input.
    println!("cargo:rustc-link-arg-bins={}", resources.display());
}

fn find_windows_rc() -> Option<PathBuf> {
    if let Some(vswhere) = env::var_os("ProgramFiles(x86)")
        .map(|folder| PathBuf::from(folder).join("Microsoft Visual Studio/Installer/vswhere.exe"))
        && let Ok(output) = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-find",
                r"**\Windows Kits\10\bin\*\x64\rc.exe",
            ])
            .output()
        && output.status.success()
        && let Some(path) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| PathBuf::from(line.trim()))
            .find(|path| path.is_file())
    {
        return Some(path);
    }
    // The SDK is not one of vswhere's products, so look where its installer
    // puts it. `WindowsSdkDir` names the SDK only inside a developer command
    // prompt, which a build run from an ordinary shell does not have.
    let mut roots = Vec::new();
    if let Some(sdk) = env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(sdk).join("bin"));
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(folder) = env::var_os(variable) {
            roots.push(PathBuf::from(folder).join("Windows Kits/10/bin"));
        }
    }
    let mut versions = roots
        .iter()
        .filter_map(|root| fs::read_dir(root).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .map(|version| version.join("x64/rc.exe"))
        .find(|path| path.is_file())
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
