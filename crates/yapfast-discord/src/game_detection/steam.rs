//! Installed Steam games supply native Linux/macOS names even when Discord's
//! catalogue only has a Windows executable. Manifests are read at most once a
//! minute, never by walking installed game files.

use super::catalog::{Catalog, GameInfo, normalize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Steamworks Common Redistributables, which Steam installs beside games.
const REDISTRIBUTABLES: u64 = 228980;

#[derive(Default, PartialEq, Eq)]
pub(super) struct Steam {
    /// The games the manifests list: their Steam id, where each is
    /// installed and its manifest's name.
    manifests: Vec<(u64, String, Box<str>)>,
    /// Where each is installed and the game it is, the most specific first.
    installed: Vec<(String, Arc<GameInfo>)>,
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > 256 * 1024 {
        return None;
    }
    let mut content = String::new();
    file.take(256 * 1024 + 1)
        .read_to_string(&mut content)
        .ok()?;
    (content.len() <= 256 * 1024).then_some(content)
}

/// Valve's text KeyValues: consume quoted key/value pairs without interpreting
/// strings as code, and handle the escaped paths Windows manifests use.
fn pairs(text: &str) -> Vec<(String, String)> {
    let mut chars = text.chars().peekable();
    let mut key = None;
    let mut result = Vec::new();
    while let Some(ch) = chars.next() {
        match ch {
            '{' | '}' => key = None,
            '/' if chars.peek() == Some(&'/') => {
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut value = String::new();
                while let Some(ch) = chars.next() {
                    match ch {
                        '"' => break,
                        '\\' => {
                            if let Some(next) = chars.next() {
                                if next != '\\' && next != '"' {
                                    value.push('\\');
                                }
                                value.push(next);
                            }
                        }
                        _ => value.push(ch),
                    }
                }
                if let Some(key) = key.take() {
                    result.push((key, value));
                } else {
                    key = Some(value);
                }
            }
            _ => {}
        }
    }
    result
}

pub(super) fn roots() -> Vec<PathBuf> {
    if let Some(root) = std::env::var_os("YAPFAST_STEAM_DIR") {
        return vec![root.into()];
    }
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    if cfg!(target_os = "windows") {
        [
            std::env::var_os("ProgramFiles(x86)"),
            std::env::var_os("ProgramFiles"),
        ]
        .into_iter()
        .flatten()
        .map(|path| PathBuf::from(path).join("Steam"))
        .collect()
    } else if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Steam")]
    } else {
        vec![
            home.join(".local/share/Steam"),
            home.join(".steam/steam"),
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        ]
    }
}

impl Steam {
    /// The games installed now, unless they're those of `earlier`. Only a
    /// change has the catalogue read, for the games Discord knows by id.
    pub(super) fn load(roots: &[PathBuf], catalog: &Catalog, earlier: &Self) -> Option<Self> {
        let mut libraries = Vec::new();
        for root in roots {
            libraries.push(root.clone());
            if let Some(vdf) = read_small(&root.join("steamapps/libraryfolders.vdf")) {
                for (key, value) in pairs(&vdf) {
                    if key == "path" {
                        libraries.push(value.into());
                    }
                }
            }
        }
        libraries = libraries
            .into_iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        libraries.sort();
        libraries.dedup();
        let mut manifests = Vec::new();
        for library in libraries {
            let Ok(files) = fs::read_dir(library.join("steamapps")) else {
                continue;
            };
            for file in files.flatten().take(10_000) {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let Some(id) = name
                    .strip_prefix("appmanifest_")
                    .and_then(|s| s.strip_suffix(".acf"))
                else {
                    continue;
                };
                let Some(id) = id.parse::<u64>().ok().filter(|&id| id != REDISTRIBUTABLES) else {
                    continue;
                };
                let Some(content) = read_small(&file.path()) else {
                    continue;
                };
                let pairs = pairs(&content);
                let Some((_, directory)) = pairs.iter().find(|(key, _)| key == "installdir") else {
                    continue;
                };
                // A manifest must not turn an arbitrary parent directory into
                // a game match. Install directories are single path components.
                if directory.is_empty()
                    || directory == "."
                    || directory == ".."
                    || directory.contains(['/', '\\'])
                {
                    continue;
                }
                let Some((_, name)) = pairs.iter().find(|(key, _)| key == "name") else {
                    continue;
                };
                if name.trim().is_empty() || name.chars().count() > 128 {
                    continue;
                }
                let path = library.join("steamapps/common").join(directory);
                // Proton and the Steam Linux Runtime install as apps too, and
                // their processes outlive a game's launch. Each has a
                // toolmanifest.vdf, which no game has.
                if path.join("toolmanifest.vdf").exists() {
                    continue;
                }
                let path = path.canonicalize().unwrap_or(path);
                manifests.push((
                    id,
                    normalize(&path.to_string_lossy(), cfg!(target_os = "windows")),
                    name.as_str().into(),
                ));
            }
        }
        // If a library has nested installation directories, the most specific
        // matching installation wins; directories are still few and bounded.
        manifests.sort_by(|(_, a, _), (_, b, _)| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        if manifests == earlier.manifests {
            return None;
        }
        let known =
            catalog.steam_games(&manifests.iter().map(|(id, ..)| *id).collect::<HashSet<_>>());
        let installed = manifests
            .iter()
            .map(|(id, path, name)| {
                let game = known.get(id).cloned().unwrap_or_else(|| {
                    Arc::new(GameInfo {
                        id: None,
                        name: name.clone(),
                    })
                });
                (path.clone(), game)
            })
            .collect();
        Some(Self {
            manifests,
            installed,
        })
    }

    pub(super) fn matches(&self, path: &str) -> Option<Arc<GameInfo>> {
        let path = normalize(path, cfg!(target_os = "windows"));
        let path = if cfg!(target_os = "linux") {
            path.strip_prefix("Z:")
                .or_else(|| path.strip_prefix("z:"))
                .unwrap_or(&path)
        } else {
            &path
        };
        self.installed
            .iter()
            .find(|(root, _)| {
                path == root
                    || path
                        .strip_prefix(root)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(|(_, game)| game.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valve_paths_and_ignores_commented_entries() {
        assert_eq!(
            pairs(
                r#""libraryfolders" { "0" { "path" "C:\\Games\\Steam" } // "path" "wrong"
        "1" { "path" "/mnt/Games" } }"#
            ),
            vec![
                ("path".into(), "C:\\Games\\Steam".into()),
                ("path".into(), "/mnt/Games".into())
            ]
        );
    }

    #[test]
    fn steam_matches_only_inside_a_games_installation() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("steamapps")).unwrap();
        fs::write(
            dir.path().join("steamapps/appmanifest_42.acf"),
            r#""AppState" { "name" "Unknown Steam Game" "installdir" "Fixture" }"#,
        )
        .unwrap();
        for (id, name, directory) in [
            ("1493710", "Proton Experimental", "Proton"),
            (
                "1628350",
                "Steam Linux Runtime 3.0 (sniper)",
                "SteamLinuxRuntime_sniper",
            ),
        ] {
            fs::write(
                dir.path().join(format!("steamapps/appmanifest_{id}.acf")),
                format!(r#""AppState" {{ "name" "{name}" "installdir" "{directory}" }}"#),
            )
            .unwrap();
            let tool = dir.path().join("steamapps/common").join(directory);
            fs::create_dir_all(&tool).unwrap();
            fs::write(tool.join("toolmanifest.vdf"), r#""manifest" {}"#).unwrap();
        }
        fs::write(
            dir.path().join("steamapps/appmanifest_228980.acf"),
            r#""AppState" { "name" "Steamworks Common Redistributables" "installdir" "Steamworks Shared" }"#,
        )
        .unwrap();
        let steam =
            Steam::load(&[dir.path().into()], &Catalog::default(), &Steam::default()).unwrap();
        assert!(
            Steam::load(&[dir.path().into()], &Catalog::default(), &steam).is_none(),
            "nothing to do while the same games are installed"
        );
        assert_eq!(
            steam.installed.len(),
            1,
            "only the game, none of Steam's tools"
        );
        // Paths, not text: a join onto a canonicalised Windows path writes its
        // separators its own way, which a replacement over the text would miss.
        let common = dir
            .path()
            .canonicalize()
            .unwrap()
            .join("steamapps")
            .join("common");
        let path = common.join("Fixture").join("game");
        assert_eq!(
            &*steam.matches(&path.to_string_lossy()).unwrap().name,
            "Unknown Steam Game"
        );
        assert!(
            steam
                .matches(&common.join("FixtureOther").join("game").to_string_lossy())
                .is_none()
        );
        assert!(
            steam
                .matches(
                    &common
                        .join("Proton")
                        .join("files")
                        .join("bin")
                        .join("wine")
                        .to_string_lossy()
                )
                .is_none()
        );
    }
}
