//! Downloads a release and puts it in place of the running program.
//!
//! Nothing calls this. It is built only with the `self-update` feature, which
//! no default or packaging build turns on, and no setting, button or command
//! line flag reaches it: the project hasn't decided whether Yapfast updates
//! itself, and a person's answer would have to come before any of it runs.
//! It is here so that deciding yes is a matter of wiring a button, and the
//! download, the check against the release's SHA256SUMS.txt and the
//! replacement are tested meanwhile.
//!
//! A release file is streamed to disk in chunks, never held whole in memory,
//! and replaces the program only after its SHA-256 matches the sum the release
//! published. The AppImage is renamed over the old file, which is atomic on one
//! filesystem, so a failed update leaves the old program running; Windows
//! runs the release's installer, which replaces the program itself.
#![allow(
    dead_code,
    reason = "no setting or button reaches self-update until the project decides on it"
)]

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{Asset, Release, Source};

/// The release's file for this install: the AppImage for an AppImage, the
/// installer for a Windows install. Other installs aren't updated from here.
pub fn asset_for(release: &Release, source: Source) -> Option<&Asset> {
    let suffix = match source {
        Source::AppImage => "-x86_64.AppImage",
        Source::Installer => "-windows-x64-setup.exe",
        Source::Manager(_) | Source::Other => return None,
    };
    release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
}

/// The sums the release published for its files.
pub fn sums_of(release: &Release) -> Option<&Asset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS.txt")
}

/// Reads `sha256sum`'s output: a hash, then the file's name, which a `*`
/// marks as read in binary mode.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (hash.len() == 64 && !name.is_empty())
                .then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// The SHA-256 of a file, read through a buffer.
pub fn sha256_of(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Whether the file at `path` is the one the release published as `name`.
pub fn verify(path: &Path, name: &str, sums: &HashMap<String, String>) -> Result<(), String> {
    let expected = sums
        .get(name)
        .ok_or_else(|| format!("The release publishes no sum for {name}."))?;
    let actual = sha256_of(path).map_err(|error| error.to_string())?;
    if &actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{name} doesn't match the sum the release published."
        ))
    }
}

/// Downloads `url` to `to`, a chunk at a time.
pub async fn download(client: &reqwest::Client, url: &str, to: &Path) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    let mut file = std::fs::File::create(to).map_err(|error| error.to_string())?;
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        file.write_all(&chunk).map_err(|error| error.to_string())?;
    }
    file.sync_all().map_err(|error| error.to_string())
}

/// Puts `new` where the running AppImage is, keeping the program's name and
/// making the file runnable. `new` must be on the same filesystem, so the
/// rename is atomic.
#[cfg(unix)]
pub fn replace_appimage(new: &Path, current: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(new, current)
}

/// Runs a downloaded installer, which replaces the program once this one quits.
pub fn run_installer(installer: &Path) -> io::Result<()> {
    std::process::Command::new(installer).spawn().map(drop)
}

/// What [`apply`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// The AppImage was replaced; the new one runs from the next start.
    Replaced(PathBuf),
    /// The installer is running; Yapfast should quit.
    Installer,
}

/// Downloads `release`'s file for this install into `folder`, checks it
/// against the release's sums, and installs it. `folder` must be on the
/// filesystem the AppImage is on, such as the AppImage's own folder.
pub async fn apply(release: &Release, source: Source, folder: &Path) -> Result<Applied, String> {
    let asset = asset_for(release, source).ok_or("This install isn't updated by Yapfast.")?;
    let sums = sums_of(release).ok_or("The release publishes no SHA256SUMS.txt.")?;
    let client = super::client()?;
    let sums = client
        .get(&sums.url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?
        .text()
        .await
        .map_err(|error| error.to_string())?;
    let sums = parse_sums(&sums);
    let part = folder.join(format!("{}.part", asset.name));
    let result = async {
        download(&client, &asset.url, &part).await?;
        verify(&part, &asset.name, &sums)
    }
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_file(&part);
        return Err(error);
    }
    match source {
        #[cfg(unix)]
        Source::AppImage => {
            let current = PathBuf::from(
                std::env::var_os("APPIMAGE").ok_or("The AppImage's path isn't known.")?,
            );
            replace_appimage(&part, &current).map_err(|error| error.to_string())?;
            Ok(Applied::Replaced(current))
        }
        Source::Installer => {
            let installer = folder.join(&asset.name);
            std::fs::rename(&part, &installer).map_err(|error| error.to_string())?;
            run_installer(&installer).map_err(|error| error.to_string())?;
            Ok(Applied::Installer)
        }
        _ => {
            let _ = std::fs::remove_file(&part);
            Err("This install isn't updated by Yapfast.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::Version;

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
    fn each_install_takes_its_own_file() {
        let release = release();
        let name = |source| asset_for(&release, source).map(|asset| asset.name.as_str());
        assert_eq!(
            name(Source::AppImage),
            Some("Yapfast-0.4.1-x86_64.AppImage")
        );
        assert_eq!(
            name(Source::Installer),
            Some("Yapfast-0.4.1-windows-x64-setup.exe")
        );
        assert_eq!(name(Source::Manager("Scoop")), None);
        assert_eq!(name(Source::Other), None);
        assert_eq!(
            sums_of(&release).map(|asset| asset.name.as_str()),
            Some("SHA256SUMS.txt")
        );
    }

    #[test]
    fn sums_are_read_as_sha256sum_writes_them() {
        let hash = "a".repeat(64);
        let text = format!(
            "{hash}  Yapfast-1.AppImage\n{hash} *binary.exe\nnot a sum line\n{} short\n",
            "b".repeat(8)
        );
        let sums = parse_sums(&text);
        assert_eq!(sums.len(), 2);
        assert_eq!(sums["Yapfast-1.AppImage"], hash);
        assert_eq!(sums["binary.exe"], hash);
    }

    #[test]
    fn a_download_must_match_its_published_sum() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("update.bin");
        std::fs::write(&file, b"hello\n").unwrap();
        // sha256 of "hello\n"
        let good = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
        assert_eq!(sha256_of(&file).unwrap(), good);
        let sums = HashMap::from([("update.bin".to_owned(), good.to_owned())]);
        assert_eq!(verify(&file, "update.bin", &sums), Ok(()));
        std::fs::write(&file, b"hello!\n").unwrap();
        assert!(verify(&file, "update.bin", &sums).is_err());
        assert!(verify(&file, "other.bin", &sums).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn replacing_the_appimage_keeps_it_runnable() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let current = folder.path().join("Yapfast.AppImage");
        let new = folder.path().join("Yapfast-new.AppImage.part");
        std::fs::write(&current, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        replace_appimage(&new, &current).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new");
        assert!(!new.exists());
        assert_eq!(
            std::fs::metadata(&current).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }
}
