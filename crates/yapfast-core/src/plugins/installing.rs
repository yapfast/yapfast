//! Asking the helper to read a plugin out of its repository, and turning what
//! it found into a review and a package the store can write.
//!
//! The reading happens in `yapfast-plugins fetch`, which runs for seconds in a
//! process of its own and prints one JSON object ([`Report`]); this side copies
//! the module and the icon it names out of the helper's scratch directory and
//! deletes that directory, so neither side leaves anything behind. Nothing here
//! runs the plugin: the manifest is parsed as JSON, and the module was already
//! checked against the ABI by the fetch that read it (`PLAN.md` §8.5).

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use yapfast_plugin_api::PluginManifest;

use crate::plugins::page::{Dependency, Review};
use crate::plugins::spawn;
use crate::plugins::store::Package;

/// What a fetch found: one plugin, or the folders of a repository that holds
/// several.
#[derive(Debug)]
pub enum Outcome {
    Plugin(Fetched),
    /// A repository whose index lists several plugins: each is fetched by its
    /// own `#path=` URL, so nothing is installed for this answer.
    Index {
        source: String,
        paths: Vec<String>,
    },
}

/// One plugin, read out of its repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    /// The id its manifest declares, which is what the store files it under.
    pub id: yapfast_plugin_api::PluginId,
    pub source: String,
    /// The folder in it, empty when the manifest is at the repository's root.
    pub path: String,
    pub reference: String,
    pub rev: String,
    /// `yapfast-plugin.json`'s text, exactly as the repository holds it.
    pub manifest: String,
    /// The module, `None` for a plugin that only changes settings or the
    /// interface.
    pub module: Option<Vec<u8>>,
    pub icon: Option<Vec<u8>>,
}

/// What the helper prints, one object tagged by `kind`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Report {
    Plugin {
        source: String,
        path: String,
        reference: String,
        rev: String,
        manifest: String,
        wasm: Option<String>,
        icon: Option<String>,
    },
    Index {
        source: String,
        paths: Vec<String>,
    },
    Error {
        error: String,
    },
}

impl Fetched {
    /// What the review dialog shows: the manifest's own words, and what the
    /// plugin may do.
    pub fn review(&self) -> Review {
        let manifest = PluginManifest::parse(&self.manifest).ok();
        Review {
            id: self.id.to_string(),
            name: manifest
                .as_ref()
                .map(|manifest| manifest.name.clone())
                .unwrap_or_default(),
            version: manifest
                .as_ref()
                .map(|manifest| manifest.version.to_string())
                .unwrap_or_default(),
            icon: None,
            source: self.source.clone(),
            path: self.path.clone(),
            commit: self.rev.clone(),
            // Only the marketplace's own entries are reviewed; a URL install is
            // an unreviewed source.
            reviewed: false,
            capabilities: manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .permissions
                        .capabilities
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            network: manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .permissions
                        .network
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            memory_pages: manifest
                .as_ref()
                .map(|manifest| manifest.limits.memory_pages)
                .unwrap_or_default(),
            dependencies: dependencies(manifest.as_ref()),
            replaces: None,
        }
    }

    /// What the store writes, once the person has accepted the review.
    pub fn package(&self) -> Package {
        Package {
            id: self.id.clone(),
            source: self.source.clone(),
            path: self.path.clone(),
            reference: if self.reference.is_empty() {
                "HEAD".to_owned()
            } else {
                self.reference.clone()
            },
            rev: self.rev.clone(),
            manifest: self.manifest.clone(),
            module: self.module.clone().unwrap_or_default(),
            icon: self.icon.clone(),
        }
    }
}

/// What a plugin's manifest requires of others, for the review to list.
fn dependencies(manifest: Option<&PluginManifest>) -> Vec<Dependency> {
    manifest
        .map(|manifest| {
            manifest
                .dependencies
                .iter()
                .map(|(id, requirement)| Dependency {
                    name: id.to_string(),
                    version: requirement.to_string(),
                    installed: false,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reads a plugin from a `.wasm` file on this computer, for trying one without
/// a repository.
///
/// A manifest beside the file is what the plugin is: it names the module, the
/// id, what the plugin may do, and when it runs. Without one, a manifest is made
/// from the file's own name — id `local.<name>`, version `0.1.0`, a 64-page
/// memory cap, and `message_create` as the one event it runs on, which needs
/// `messages.read`. That is the least a bare module can be given that does
/// something once the person enables it: the capability it declares is listed on
/// its row, and installing never enables it.
pub fn from_wasm(path: &Path) -> Result<Fetched, String> {
    let folder = path.parent().unwrap_or(Path::new("."));
    let beside = folder.join(crate::plugins::MANIFEST);
    let text = match beside.is_file() {
        true => std::fs::read_to_string(&beside)
            .map_err(|error| format!("could not read {}: {error}", beside.display()))?,
        false => made_from(path)?,
    };
    let manifest = PluginManifest::parse(&text)
        .map_err(|error| format!("{} can't be used: {error}", beside.display()))?;
    // The manifest is what runs, so it decides which module to read: the file
    // the person picked only stands in for a plugin that brought no manifest.
    let module_path = manifest
        .wasm
        .as_ref()
        .map(|wasm| folder.join(wasm))
        .unwrap_or_else(|| path.to_path_buf());
    let module = std::fs::read(&module_path)
        .map_err(|error| format!("could not read {}: {error}", module_path.display()))?;
    if !module.starts_with(b"\0asm") {
        return Err(format!(
            "{} isn't a WebAssembly module: it doesn't start with the `\\0asm` magic",
            module_path.display()
        ));
    }
    Ok(Fetched {
        id: manifest.id,
        // The folder's own path, so a later read of the same file finds it.
        source: format!("file://{}", folder.display()),
        path: String::new(),
        reference: String::new(),
        rev: String::new(),
        manifest: text,
        module: Some(module),
        icon: None,
    })
}

/// A manifest for a module that brought none: its own name, and the least it
/// can be given that runs once it is enabled.
fn made_from(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("plugin");
    let name: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-');
    let module = path
        .file_name()
        .and_then(|file| file.to_str())
        .ok_or_else(|| format!("{} isn't a name this can read", path.display()))?;
    let id = format!("local.{}", if name.is_empty() { "plugin" } else { name });
    // The manifest is written by hand rather than through a struct so it reads
    // as what a person would write, settings and all, in the store.
    Ok(format!(
        r#"{{
  "api": {api},
  "id": "{id}",
  "name": "{title}",
  "version": "0.1.0",
  "description": "A plugin from this computer.",
  "wasm": "{module}",
  "limits": {{ "memory_pages": 64 }},
  "activation": {{ "on": ["event:message_create"] }},
  "permissions": {{ "capabilities": ["messages.read"], "network": [] }}
}}
"#,
        api = crate::plugins::API,
        title = title(stem),
    ))
}

/// A file's stem as a name a person reads: `word-filter` is "Word Filter".
fn title(stem: &str) -> String {
    stem.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(letters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads one plugin out of its repository with the helper.
pub fn fetch(url: &str) -> Result<Outcome, String> {
    let program = spawn::program().map_err(|error| format!("{error:#}"))?;
    let output = Command::new(&program)
        .arg("fetch")
        .arg(url)
        .output()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&printed).map_err(|_| {
        // The helper prints an error object for what it could not fetch, so a
        // line that isn't one comes from the helper failing to start at all.
        if output.status.success() {
            "the plugin helper printed something that isn't a fetch report".to_owned()
        } else {
            "the plugin helper couldn't fetch that repository".to_owned()
        }
    })?;
    match report {
        Report::Error { error } => Err(error),
        Report::Index { source, paths } => Ok(Outcome::Index { source, paths }),
        Report::Plugin {
            source,
            path,
            reference,
            rev,
            manifest,
            wasm,
            icon,
        } => {
            // The scratch directory held the module and the icon; nothing else
            // needs it once they are here, whoever named the two files.
            let scratch = wasm.clone().or_else(|| icon.clone());
            let module = match &wasm {
                Some(file) => Some(read_and_remove(file)?),
                None => None,
            };
            let icon = match &icon {
                Some(file) => Some(read_and_remove(file)?),
                None => None,
            };
            if let Some(file) = &scratch {
                remove_scratch(file);
            }
            let id = PluginManifest::parse(&manifest)
                .map(|manifest| manifest.id)
                .map_err(|error| format!("its yapfast-plugin.json can't be used: {error}"))?;
            Ok(Outcome::Plugin(Fetched {
                id,
                source,
                path,
                reference,
                rev,
                manifest,
                module,
                icon,
            }))
        }
    }
}

/// One file the helper wrote, and its removal.
fn read_and_remove(file: &str) -> Result<Vec<u8>, String> {
    std::fs::read(file).map_err(|error| format!("could not read {file}: {error}"))
}

/// Removes the directory the helper wrote a file into: its parent is the scratch
/// directory, and the helper leaves it for this side to delete.
fn remove_scratch(file: &str) {
    let Some(directory) = Path::new(file).parent() else {
        return;
    };
    if directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".yapfast-plugin-"))
        || directory.join("plugin.wasm").exists()
        || directory.join("yapfast-plugin.json").exists()
    {
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_is_read_as_a_review() {
        let fetched = Fetched {
            id: yapfast_plugin_api::PluginId::parse("dev.local.echo").expect("a plugin id"),
            source: "https://example.org/ada/echo".to_owned(),
            path: "plugins/echo".to_owned(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest: r#"{
                "api": 1,
                "id": "dev.local.echo",
                "name": "Echo",
                "version": "0.1.0",
                "wasm": "plugin.wasm",
                "limits": { "memory_pages": 64 },
                "permissions": { "capabilities": ["messages.read"], "network": ["api.example.com"] }
            }"#
            .to_owned(),
            module: Some(vec![0, 97, 115, 109]),
            icon: None,
        };
        let review = fetched.review();
        assert_eq!(review.id, "dev.local.echo");
        assert_eq!(review.version, "0.1.0");
        assert_eq!(review.memory_pages, 64);
        assert_eq!(review.capabilities, ["messages.read"]);
        assert_eq!(review.network, ["api.example.com"]);
        // A URL install is never the marketplace's reviewed listing.
        assert!(!review.reviewed);
        assert_eq!(review.path, "plugins/echo");
        assert_eq!(fetched.package().path, "plugins/echo");
        assert_eq!(fetched.package().module.len(), 4);
    }

    #[test]
    fn a_manifest_that_makes_no_sense_does_not_panic_the_review() {
        let fetched = Fetched {
            manifest: "{".to_owned(),
            ..Fetched {
                id: yapfast_plugin_api::PluginId::parse("dev.local.echo").expect("a plugin id"),
                source: "https://example.org/x".to_owned(),
                path: String::new(),
                reference: String::new(),
                rev: "0".repeat(40),
                manifest: String::new(),
                module: None,
                icon: None,
            }
        };
        let review = fetched.review();
        // The id came from the manifest the helper validated, so the review still
        // knows which plugin it is about.
        assert_eq!(review.id, "dev.local.echo");
        assert!(review.capabilities.is_empty());
    }
}
