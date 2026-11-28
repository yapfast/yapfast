//! Links the Windows executable's icon and version information with the
//! available resource compiler. Tells the program which build it is.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows");
    println!("cargo:rerun-if-changed=assets/icons/app/yapfast.ico");
    build_info();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}

/// Hands src/build_info.rs the commit and its date, and whether this is a
/// release build. Nix's sandbox has no Git, so nix/package.nix gives the
/// first two in `YAPFAST_COMMIT` and `YAPFAST_COMMIT_DATE`; a build in a
/// checkout asks Git. Without either, the build names no commit. Only a
/// release's packaging sets `YAPFAST_RELEASE`: every other build, such as
/// `cargo run`, is a development build.
fn build_info() {
    for name in ["YAPFAST_COMMIT", "YAPFAST_COMMIT_DATE", "YAPFAST_RELEASE"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    // Only this directory's own repository counts: source unpacked inside
    // another one, as a package build may do, isn't at that one's commit.
    // `--show-prefix` is empty at a repository's top, also in a worktree.
    let checkout = git(&["rev-parse", "--show-prefix"]).is_some_and(|prefix| prefix.is_empty());
    // A new commit or a checkout moves HEAD, and appends to its log, which
    // also changes when HEAD stays on a branch that gains a commit. Edits to
    // files aren't watched, so a build doesn't recompile the program for
    // them: the commit named is the one the checkout is on, with or without
    // changes on top.
    if checkout {
        for path in ["HEAD", "logs/HEAD"] {
            if let Some(path) = git(&["rev-parse", "--git-path", path]).map(PathBuf::from)
                && path.exists()
            {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    let commit = env::var("YAPFAST_COMMIT")
        .ok()
        .filter(|commit| !commit.is_empty())
        .or_else(|| git(&["rev-parse", "HEAD"]).filter(|_| checkout));
    let date = env::var("YAPFAST_COMMIT_DATE")
        .ok()
        .filter(|date| !date.is_empty())
        .or_else(|| {
            // UTC, as Nix's `lastModifiedDate` is, so a commit has one date
            // wherever it is built.
            Command::new("git")
                .args(["log", "-1", "--date=format-local:%Y-%m-%d", "--format=%cd"])
                .env("TZ", "UTC")
                .output()
                .ok()
                .filter(|output| checkout && output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|date| date.trim().to_owned())
        });
    if let Some(commit) = commit.filter(|commit| !commit.is_empty()) {
        println!("cargo:rustc-env=YAPFAST_BUILD_COMMIT={commit}");
    }
    if let Some(date) = date.filter(|date| !date.is_empty()) {
        println!("cargo:rustc-env=YAPFAST_BUILD_DATE={date}");
    }
    if env::var_os("YAPFAST_RELEASE").is_some_and(|release| !release.is_empty()) {
        println!("cargo:rustc-env=YAPFAST_BUILD_RELEASE=1");
    }
}

/// What `git` prints for `arguments`, when it runs here and succeeds.
fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned())
}

fn windows_resources() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let resources = out.join("yapfast.res");
    let script = out.join("yapfast.rc");
    let version = concat!(
        env!("CARGO_PKG_VERSION_MAJOR"),
        ",",
        env!("CARGO_PKG_VERSION_MINOR"),
        ",",
        env!("CARGO_PKG_VERSION_PATCH"),
        ",0"
    );
    // Expand the package version here so the resource compiler needs no preprocessor.
    let source = include_str!("packaging/windows/yapfast.rc")
        .replace("@VERSION_NUMBERS@", version)
        .replace("@VERSION_STRING@", env!("CARGO_PKG_VERSION"));
    fs::write(&script, source).expect("could not write the Windows resource script");
    let status = match Command::new("llvm-rc")
        .current_dir("packaging/windows")
        .args(["-no-preprocess", "-FO"])
        .arg(&resources)
        .arg(&script)
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
                .arg(&script)
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
