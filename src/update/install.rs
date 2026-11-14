//! Downloads a release and puts it in place of the running program, when a
//! person presses Update and restart in Settings' About page.
//!
//! Only two installs replace themselves, because only they own their files:
//!
//! - **A Linux AppImage** is one file the person can write to. The release's
//!   AppImage is downloaded next to it, checked against the release's
//!   `SHA256SUMS.txt`, started once with `--smoke-test` to see that it runs on
//!   this computer, and renamed over the old file, which is atomic on one
//!   filesystem. The running copy keeps working from the file it has open.
//! - **Windows' installer** put the program in a folder with its uninstaller
//!   beside it. The release's installer is downloaded to the temporary folder
//!   and checked; after Yapfast has quit, a hidden `cmd` runs it silently,
//!   waits, and starts the program again, as a silent Inno Setup run starts
//!   nothing itself.
//!
//! Every other copy (a package manager's, Flatpak, a portable ZIP, a build from
//! source) is left alone, with the reason. A release file is streamed to disk in
//! chunks, hashed as it arrives, and used only when its SHA-256 matches the sum
//! the release published; a failure at any step leaves the old program
//! running and the downloaded file removed.

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

use sha2::{Digest, Sha256};
use yapfast_core::runtime::BackgroundRuntime;

use super::{Asset, Release, Source};

/// What an update will replace, and with which of the release's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The AppImage at this path.
    AppImage { asset: Asset, current: PathBuf },
    /// The installed program at this path.
    Installer { asset: Asset, program: PathBuf },
}

impl Target {
    pub fn asset(&self) -> &Asset {
        match self {
            Target::AppImage { asset, .. } | Target::Installer { asset, .. } => asset,
        }
    }

    /// Where the download goes until it is checked: beside the AppImage, so the
    /// rename stays on one filesystem; in the temporary folder for the installer.
    pub fn part(&self) -> PathBuf {
        match self {
            Target::AppImage { current, .. } => current.with_file_name(".yapfast-update.part"),
            Target::Installer { asset, .. } => std::env::temp_dir()
                .join("yapfast-update")
                .join(&asset.name),
        }
    }
}

/// The release's file for this install. The reason it has none is the text
/// About shows beside Update and restart's absence.
pub fn target_for(
    release: &Release,
    source: Source,
    program: &Path,
    appimage: Option<&Path>,
) -> Result<Target, String> {
    let suffix = match source {
        Source::AppImage if cfg!(target_arch = "x86_64") => "-x86_64.AppImage",
        Source::Installer if cfg!(target_arch = "x86_64") => "-windows-x64-setup.exe",
        Source::AppImage | Source::Installer => {
            return Err("There's no download for your system yet.".into());
        }
        Source::Manager(name) => return Err(format!("{name} updates this copy.")),
        Source::Other => {
            return Err(
                "This copy can't update itself. Get the new version from the release page.".into(),
            );
        }
    };
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .ok_or_else(|| match source {
            Source::Installer => "There's no Windows installer for this version yet.".to_owned(),
            _ => "There's no AppImage for this version yet.".to_owned(),
        })?
        .clone();
    sums_of(release).ok_or("This release doesn't publish SHA256SUMS.txt.")?;
    match source {
        Source::AppImage => {
            let current = appimage
                .ok_or("The AppImage's own path isn't known.")?
                .to_path_buf();
            let folder = current.parent().ok_or("The AppImage has no folder.")?;
            if !writable(folder) {
                return Err("Yapfast can't write to the AppImage's folder.".into());
            }
            Ok(Target::AppImage { asset, current })
        }
        _ => {
            let folder = program.parent().ok_or("Yapfast has no folder.")?;
            // The installer left its uninstaller there; a folder without one
            // was unpacked by hand or installed by something else.
            if !folder.join("unins000.exe").is_file() {
                return Err(
                    "This copy can't update itself. Get the new version from the release page."
                        .into(),
                );
            }
            if !writable(folder) {
                return Err("Yapfast can't write to its own folder.".into());
            }
            Ok(Target::Installer {
                asset,
                program: program.to_path_buf(),
            })
        }
    }
}

/// [`target_for`] for the running program.
pub fn target(release: &Release) -> Result<Target, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let mut source = super::source();
    // An install the person put in a folder of their choosing is still the
    // installer's, which its uninstaller beside the program shows.
    if cfg!(windows)
        && source == Source::Other
        && program
            .parent()
            .is_some_and(|folder| folder.join("unins000.exe").is_file())
    {
        source = Source::Installer;
    }
    target_for(release, source, &program, appimage.as_deref())
}

/// Whether a file can be made in `folder`.
fn writable(folder: &Path) -> bool {
    tempfile::Builder::new()
        .prefix(".yapfast-write-test")
        .tempfile_in(folder)
        .is_ok()
}

/// The sums the release published for its files.
fn sums_of(release: &Release) -> Option<&Asset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS.txt")
}

/// Reads `sha256sum`'s output: a hash, then the file's name, which a `*`
/// marks as read in binary mode.
fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// Where an update has got to, for About's progress line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Starting,
    Downloading,
    Testing,
    Installing,
}

/// Progress the update task writes and About reads on a timer, so no event
/// loop call comes from the download.
#[derive(Default)]
pub struct Progress {
    stage: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    fn stage(&self, stage: Stage) {
        self.stage.store(stage as u8, Ordering::Relaxed);
    }

    /// The stage now, and how much of the download is done (0 to 1), or
    /// nothing when it isn't known.
    pub fn read(&self) -> (Stage, Option<f32>) {
        let stage = match self.stage.load(Ordering::Relaxed) {
            1 => Stage::Downloading,
            2 => Stage::Testing,
            3 => Stage::Installing,
            _ => Stage::Starting,
        };
        let total = self.total.load(Ordering::Relaxed);
        let done = self.done.load(Ordering::Relaxed);
        let fraction = (total > 0).then(|| (done as f64 / total as f64).min(1.0) as f32);
        (stage, fraction)
    }
}

/// Reads the sums file's text, small enough to hold.
async fn fetch_sums(
    client: &reqwest::Client,
    url: &str,
) -> Result<HashMap<String, String>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > 1 << 20)
    {
        return Err("The release's sums are too large to be right.".into());
    }
    let text = response
        .text()
        .await
        .map_err(|error| error.without_url().to_string())?;
    Ok(parse_sums(&text))
}

/// Downloads `url` to `to` a chunk at a time, hashing as it goes; returns the
/// SHA-256 as lower-case hex.
async fn download(
    client: &reqwest::Client,
    url: &str,
    to: &Path,
    expected_size: u64,
    progress: &Progress,
) -> Result<String, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    progress.total.store(
        response.content_length().unwrap_or(expected_size),
        Ordering::Relaxed,
    );
    progress.done.store(0, Ordering::Relaxed);
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let mut file = std::fs::File::create(to).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        progress
            .done
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Runs the downloaded AppImage's `--smoke-test` to see that it starts here,
/// giving up after a minute.
#[cfg(unix)]
fn smoke_test(program: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    let mut child = std::process::Command::new(program)
        .arg("--smoke-test")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("The new version wouldn't start: {error}."))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("The new version didn't pass its start-up check.".into()),
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(60) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The new version's start-up check took too long.".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// What [`prepare`] left ready, for the program to finish once it has quit.
#[derive(Debug, PartialEq, Eq)]
pub enum Prepared {
    /// The AppImage was replaced; start this file.
    #[cfg_attr(not(unix), allow(dead_code))]
    Replaced(PathBuf),
    /// The installer is checked and waiting; running it replaces the program.
    Installer {
        installer: PathBuf,
        program: PathBuf,
    },
}

/// Downloads `target`'s file, checks it against the release's sums, and gets
/// it ready: an AppImage is tested and put in place, an installer waits. On a
/// failure the downloaded file is removed.
pub async fn prepare(
    runtime: &BackgroundRuntime,
    client: &reqwest::Client,
    release: &Release,
    target: &Target,
    progress: Arc<Progress>,
) -> Result<Prepared, String> {
    let part = target.part();
    let result = prepare_in(runtime, client, release, target, &part, &progress).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

async fn prepare_in(
    runtime: &BackgroundRuntime,
    client: &reqwest::Client,
    release: &Release,
    target: &Target,
    part: &Path,
    progress: &Arc<Progress>,
) -> Result<Prepared, String> {
    let asset = target.asset();
    progress.stage(Stage::Starting);
    let sums = sums_of(release).ok_or("This release doesn't publish SHA256SUMS.txt.")?;
    let sums = fetch_sums(client, &sums.url).await?;
    let expected = sums.get(&asset.name).ok_or_else(|| {
        format!(
            "The download of {} couldn't be verified, so it wasn't installed.",
            asset.name
        )
    })?;
    progress.stage(Stage::Downloading);
    let actual = download(client, &asset.url, part, asset.size, progress).await?;
    if &actual != expected {
        return Err(format!(
            "{} doesn't match the sum the release published.",
            asset.name
        ));
    }
    match target {
        Target::AppImage { current, .. } => {
            progress.stage(Stage::Testing);
            #[cfg(unix)]
            {
                let (new, to) = (part.to_path_buf(), current.clone());
                runtime
                    .spawn_blocking(move || {
                        smoke_test(&new)?;
                        // The same folder, so the rename is atomic: the old
                        // program is there or the new one, never half of either.
                        std::fs::rename(&new, &to).map_err(|error| error.to_string())
                    })
                    .await
                    .map_err(|_| "The update stopped partway.".to_owned())??;
                progress.stage(Stage::Installing);
                Ok(Prepared::Replaced(current.clone()))
            }
            #[cfg(not(unix))]
            {
                let _ = (runtime, current);
                Err("AppImages are for Linux.".into())
            }
        }
        Target::Installer { asset, program } => {
            let installer = part.with_file_name(&asset.name);
            std::fs::rename(part, &installer).map_err(|error| error.to_string())?;
            progress.stage(Stage::Installing);
            Ok(Prepared::Installer {
                installer,
                program: program.clone(),
            })
        }
    }
}

/// The command line `cmd` runs on Windows: the installer, silent and waited
/// for, then the program again with the arguments it was started with (the
/// data folder it used, say), then the installer's file removed. `None` when a
/// path or argument holds a character `cmd` would read as more than a name
/// (`"` can't be in a Windows path; `%` would expand a variable).
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only Windows runs an installer")
)]
pub fn windows_command(
    installer: &Path,
    program: &Path,
    arguments: &[String],
    log: &Path,
) -> Option<String> {
    let quoted = |path: &Path| {
        let text = path.to_str()?;
        (!text.contains(['"', '%', '\r', '\n'])).then(|| format!("\"{text}\""))
    };
    let (installer, program, log) = (quoted(installer)?, quoted(program)?, quoted(log)?);
    let mut program = program;
    for argument in arguments {
        let argument = quoted(Path::new(argument))?;
        program.push(' ');
        program.push_str(&argument);
    }
    // Inno Setup's own switches: no pages, no message boxes, no restart, and
    // the running program closed (it has quit already, so it's belt and braces).
    let log_switch = format!("/LOG={log}");
    Some(format!(
        "start \"\" /wait {installer} /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS {log_switch} & start \"\" {program} & del /q {installer}"
    ))
}

/// Runs the installer once Yapfast has quit, then starts the program again.
/// The caller returns right after, so the files can be replaced.
#[cfg(windows)]
pub fn run_installer(installer: &Path, program: &Path, arguments: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let log = installer.with_extension("log");
    let line = windows_command(installer, program, arguments, &log)
        .ok_or("The update's path has a character the installer can't be started with.")?;
    // `/S` makes `cmd` strip the outer quotes only, leaving the inner ones.
    std::process::Command::new("cmd.exe")
        .args(["/D", "/S", "/C"])
        .raw_arg(format!("\"{line}\""))
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| error.to_string())
}

/// Only Windows has an installer to run.
#[cfg(not(windows))]
pub fn run_installer(_: &Path, _: &Path, _: &[String]) -> Result<(), String> {
    Err("Installers are for Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::Version;
    use yapfast_core::runtime::BackgroundRuntime;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            url: format!("https://example.test/{name}"),
            size: 1,
        }
    }

    fn release() -> Release {
        Release {
            version: Version::parse("0.4.1").unwrap(),
            tag: "v0.4.1".into(),
            page: String::new(),
            notes: String::new(),
            assets: vec![
                asset("Yapfast-0.4.1-x86_64.AppImage"),
                asset("Yapfast-0.4.1-x86_64.AppImage.zsync"),
                asset("Yapfast-0.4.1-windows-x64-setup.exe"),
                asset("Yapfast-0.4.1-windows-x64.zip"),
                asset("SHA256SUMS.txt"),
            ],
        }
    }

    #[test]
    fn only_the_installs_that_own_their_files_are_updated() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("Yapfast.AppImage");
        let program = folder.path().join("yapfast.exe");
        let release = release();
        let reason = |source| {
            target_for(&release, source, &program, Some(&app)).expect_err("not updated here")
        };
        assert!(reason(Source::Manager("Scoop")).contains("Scoop"));
        assert!(reason(Source::Other).contains("can't update itself"));
        if !cfg!(target_arch = "x86_64") {
            return;
        }
        // An AppImage in a folder that can be written to.
        let target = target_for(&release, Source::AppImage, &program, Some(&app)).unwrap();
        assert_eq!(target.asset().name, "Yapfast-0.4.1-x86_64.AppImage");
        assert_eq!(target.part(), folder.path().join(".yapfast-update.part"));
        assert!(
            target_for(&release, Source::AppImage, &program, None)
                .unwrap_err()
                .contains("path")
        );
        // An installed program needs the installer's uninstaller beside it.
        assert!(
            reason(Source::Installer).contains("can't update itself"),
            "no uninstaller"
        );
        std::fs::write(folder.path().join("unins000.exe"), b"").unwrap();
        let target = target_for(&release, Source::Installer, &program, None).unwrap();
        assert_eq!(target.asset().name, "Yapfast-0.4.1-windows-x64-setup.exe");
        assert!(
            target
                .part()
                .ends_with("yapfast-update/Yapfast-0.4.1-windows-x64-setup.exe")
                || target
                    .part()
                    .ends_with(r"yapfast-update\Yapfast-0.4.1-windows-x64-setup.exe")
        );
    }

    #[test]
    fn a_release_without_its_file_or_sums_is_not_installed() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("Yapfast.AppImage");
        let program = folder.path().join("yapfast.exe");
        std::fs::write(folder.path().join("unins000.exe"), b"").unwrap();
        if !cfg!(target_arch = "x86_64") {
            return;
        }
        let mut release = release();
        release
            .assets
            .retain(|asset| !asset.name.ends_with("-setup.exe"));
        assert!(
            target_for(&release, Source::Installer, &program, None)
                .unwrap_err()
                .contains("no Windows installer")
        );
        release
            .assets
            .retain(|asset| asset.name != "SHA256SUMS.txt");
        assert!(
            target_for(&release, Source::AppImage, &program, Some(&app))
                .unwrap_err()
                .contains("SHA256SUMS")
        );
    }

    #[test]
    fn sums_are_read_as_sha256sum_writes_them() {
        let hash = "a".repeat(64);
        let text = format!(
            "{hash}  Yapfast-1.AppImage\n{hash} *binary.exe\nnot a sum line\n{} short\n{} nothex.bin\n",
            "b".repeat(8),
            "z".repeat(64),
        );
        let sums = parse_sums(&text);
        assert_eq!(sums.len(), 2);
        assert_eq!(sums["Yapfast-1.AppImage"], hash);
        assert_eq!(sums["binary.exe"], hash);
    }

    #[test]
    fn the_windows_command_quotes_every_path_and_refuses_odd_ones() {
        let line = windows_command(
            Path::new(r"C:\Temp\yapfast-update\Setup.exe"),
            Path::new(r"C:\Users\Jörg Müller\AppData\Local\Programs\Yapfast\yapfast.exe"),
            &[],
            Path::new(r"C:\Temp\yapfast-update\Setup.log"),
        )
        .unwrap();
        assert_eq!(
            line,
            "start \"\" /wait \"C:\\Temp\\yapfast-update\\Setup.exe\" /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /LOG=\"C:\\Temp\\yapfast-update\\Setup.log\" & start \"\" \"C:\\Users\\Jörg Müller\\AppData\\Local\\Programs\\Yapfast\\yapfast.exe\" & del /q \"C:\\Temp\\yapfast-update\\Setup.exe\""
        );
        for odd in [r"C:\100%\yapfast.exe", "C:\\a\"b\\yapfast.exe"] {
            assert_eq!(
                windows_command(Path::new("s.exe"), Path::new(odd), &[], Path::new("s.log")),
                None
            );
        }
        // The arguments the program had come back with it, quoted the same way.
        let line = windows_command(
            Path::new("s.exe"),
            Path::new("y.exe"),
            &["--data-dir".into(), r"D:\My data".into()],
            Path::new("s.log"),
        )
        .unwrap();
        assert!(
            line.contains(r#"start "" "y.exe" "--data-dir" "D:\My data" & del"#),
            "{line}"
        );
        assert_eq!(
            windows_command(
                Path::new("s.exe"),
                Path::new("y.exe"),
                &["100%".into()],
                Path::new("s.log")
            ),
            None
        );
    }

    /// A small HTTP server on this computer that answers each request from `files`.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0u8; 2048];
                let read = stream.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..read]);
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .trim_start_matches('/')
                    .to_owned();
                match files.iter().find(|(name, _)| *name == path) {
                    Some((_, body)) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
            }
        });
        format!("http://{address}")
    }

    /// The installer's download folder is shared, so the tests that use it
    /// take turns.
    static INSTALLER_FOLDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn client() -> reqwest::Client {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        reqwest::Client::new()
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// A release whose AppImage and sums are served from `base`.
    fn served_release(base: &str) -> Release {
        let mut release = release();
        for asset in &mut release.assets {
            asset.url = format!("{base}/{}", asset.name);
        }
        release
    }

    #[test]
    fn a_download_that_matches_its_sum_is_installed() {
        let _turn = INSTALLER_FOLDER.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = BackgroundRuntime::new().unwrap();
        runtime.block_on(matching_download(&runtime));
    }

    async fn matching_download(runtime: &BackgroundRuntime) {
        let new = b"#!/bin/sh\nexit 0\n".to_vec();
        let name = "Yapfast-0.4.1-windows-x64-setup.exe";
        let sums = format!("{}  {name}\n", sha(&new)).into_bytes();
        let base = serve(vec![
            ("Yapfast-0.4.1-windows-x64-setup.exe", new.clone()),
            ("SHA256SUMS.txt", sums),
        ]);
        let release = served_release(&base);
        let folder = tempfile::tempdir().unwrap();
        let program = folder.path().join("yapfast.exe");
        let target = Target::Installer {
            asset: release
                .assets
                .iter()
                .find(|asset| asset.name == name)
                .unwrap()
                .clone(),
            program: program.clone(),
        };
        let progress = Arc::new(Progress::default());
        let prepared = prepare(runtime, &client(), &release, &target, progress.clone())
            .await
            .unwrap();
        let Prepared::Installer { installer, .. } = prepared else {
            panic!("an installer was expected");
        };
        assert_eq!(std::fs::read(&installer).unwrap(), new);
        let (stage, fraction) = progress.read();
        assert_eq!(stage, Stage::Installing);
        assert_eq!(fraction, Some(1.0));
        let _ = std::fs::remove_dir_all(installer.parent().unwrap());
    }

    #[test]
    fn a_download_that_does_not_match_leaves_nothing_behind() {
        let _turn = INSTALLER_FOLDER.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = BackgroundRuntime::new().unwrap();
        runtime.block_on(mismatched_download(&runtime));
    }

    async fn mismatched_download(runtime: &BackgroundRuntime) {
        let name = "Yapfast-0.4.1-windows-x64-setup.exe";
        let sums = format!("{}  {name}\n", sha(b"what was published")).into_bytes();
        let base = serve(vec![
            (
                "Yapfast-0.4.1-windows-x64-setup.exe",
                b"something else".to_vec(),
            ),
            ("SHA256SUMS.txt", sums),
        ]);
        let release = served_release(&base);
        let folder = tempfile::tempdir().unwrap();
        let target = Target::Installer {
            asset: release
                .assets
                .iter()
                .find(|a| a.name == name)
                .unwrap()
                .clone(),
            program: folder.path().join("yapfast.exe"),
        };
        let error = prepare(
            runtime,
            &client(),
            &release,
            &target,
            Arc::new(Progress::default()),
        )
        .await
        .unwrap_err();
        assert!(error.contains("doesn't match"), "{error}");
        assert!(!target.part().exists());
        // A release that publishes no sum for the file isn't trusted either.
        let base = serve(vec![
            ("Yapfast-0.4.1-windows-x64-setup.exe", b"x".to_vec()),
            ("SHA256SUMS.txt", b"".to_vec()),
        ]);
        let release = served_release(&base);
        let error = prepare(
            runtime,
            &client(),
            &release,
            &target,
            Arc::new(Progress::default()),
        )
        .await
        .unwrap_err();
        assert!(error.contains("couldn't be verified"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_that_runs_replaces_the_old_one_and_one_that_does_not_is_dropped() {
        let runtime = BackgroundRuntime::new().unwrap();
        runtime.block_on(appimage_download(&runtime));
    }

    #[cfg(unix)]
    async fn appimage_download(runtime: &BackgroundRuntime) {
        use std::os::unix::fs::PermissionsExt;
        if !cfg!(target_arch = "x86_64") {
            return;
        }
        let name = "Yapfast-0.4.1-x86_64.AppImage";
        let folder = tempfile::tempdir().unwrap();
        let current = folder.path().join("Yapfast.AppImage");
        std::fs::write(&current, b"old").unwrap();
        for (script, works) in [
            (&b"#!/bin/sh\nexit 0\n"[..], true),
            (&b"#!/bin/sh\nexit 3\n"[..], false),
        ] {
            let sums = format!("{}  {name}\n", sha(script)).into_bytes();
            let base = serve(vec![
                ("Yapfast-0.4.1-x86_64.AppImage", script.to_vec()),
                ("SHA256SUMS.txt", sums),
            ]);
            let release = served_release(&base);
            let target = Target::AppImage {
                asset: release
                    .assets
                    .iter()
                    .find(|a| a.name == name)
                    .unwrap()
                    .clone(),
                current: current.clone(),
            };
            let result = prepare(
                runtime,
                &client(),
                &release,
                &target,
                Arc::new(Progress::default()),
            )
            .await;
            assert!(!target.part().exists());
            if works {
                assert_eq!(result, Ok(Prepared::Replaced(current.clone())));
                assert_eq!(std::fs::read(&current).unwrap(), script);
                assert_eq!(
                    std::fs::metadata(&current).unwrap().permissions().mode() & 0o111,
                    0o111
                );
            } else {
                assert!(result.unwrap_err().contains("start-up check"));
                // The program that works is still there.
                assert_ne!(std::fs::read(&current).unwrap(), script);
            }
        }
    }
}
