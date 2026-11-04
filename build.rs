//! Gives the Windows executable its icon: compiles packaging/windows/yapfast.rc
//! with LLVM's resource compiler and links the result. Packs Twemoji's pictures
//! into the program. On Linux, lets a plain `cargo build` link rust-skia's own
//! Skia archive beside libwebrtc.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows");
    println!("cargo:rerun-if-env-changed=SKIA_BINARIES_URL");
    println!("cargo:rerun-if-changed=vendor/twemoji/72x72");
    if env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        pack_twemoji();
    }
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => windows_icon(),
        Ok("linux") => linux_skia(),
        _ => {}
    }
}

/// Writes Twemoji's pictures into one file the program carries: a count, then
/// each picture's name, where it starts and how long it is, then the names and
/// the pictures. `src/twemoji.rs` reads it, and finds one by its name.
fn pack_twemoji() {
    let folder = Path::new("vendor/twemoji/72x72");
    let mut pictures: Vec<(String, Vec<u8>)> = fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", folder.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .map(|path| {
            let name = path
                .file_stem()
                .expect("a picture's name")
                .to_string_lossy()
                .to_ascii_lowercase();
            let bytes = fs::read(&path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
            (name, bytes)
        })
        .collect();
    assert!(
        !pictures.is_empty(),
        "vendor/twemoji/72x72 holds no pictures"
    );
    // Names sort, so the program can look one up by binary search.
    pictures.sort_by(|left, right| left.0.cmp(&right.0));
    // An entry is its name's length, u16, then where its picture starts and how
    // long it is, u32 each.
    const ENTRY: usize = 10;
    let header = 4 + pictures.len() * ENTRY;
    let names_length: usize = pictures.iter().map(|(name, _)| name.len()).sum();
    let mut pictures_start = header + names_length;
    let mut packed = Vec::new();
    packed.extend_from_slice(&(pictures.len() as u32).to_le_bytes());
    for (name, bytes) in &pictures {
        assert!(name.len() <= u16::MAX as usize && u32::try_from(bytes.len()).is_ok());
        packed.extend_from_slice(&(name.len() as u16).to_le_bytes());
        packed.extend_from_slice(&(pictures_start as u32).to_le_bytes());
        packed.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        pictures_start += bytes.len();
    }
    for (name, _) in &pictures {
        packed.extend_from_slice(name.as_bytes());
    }
    for (_, bytes) in &pictures {
        packed.extend_from_slice(bytes);
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("twemoji.bin");
    fs::write(&output, packed).expect("writing the packed pictures");
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
