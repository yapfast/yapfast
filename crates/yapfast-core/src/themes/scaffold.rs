//! `yapfast theme new`: a theme's folder, made from Yapfast's template
//! (`themes/template/`, built in), with its id, name and author filled in,
//! one variant, dark or light, and the template's files that variant names.
//! A new theme can start from another theme's colours instead of the
//! template's.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::package::{self, LICENCE_PLACEHOLDER, Mode, Package};
use super::{MANIFEST, check};

include!(concat!(env!("OUT_DIR"), "/theme_template.rs"));

/// What a new theme's variant starts as, instead of the template's: another
/// theme's base and colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    /// What it was, as the person named it, for the description.
    pub from: String,
    pub base: String,
    pub dark: bool,
    /// Colours over `base`, by their name in
    /// [`COLOURS`](super::package::COLOURS), as
    /// `theme.json` writes them: `#rrggbb`, `#rrggbbaa` or a gradient.
    pub colours: serde_json::Map<String, Value>,
}

/// A theme to make.
#[derive(Clone, Debug, Default)]
pub struct NewTheme {
    pub name: String,
    /// Reverse-DNS; [`default_id`] when unset.
    pub id: Option<String>,
    pub author: String,
    pub light: bool,
    pub start: Option<Start>,
}

/// A template file's bytes, by its path in the folder.
pub fn template_file(path: &str) -> Option<&'static [u8]> {
    TEMPLATE
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

/// An id for a theme: `io.github.<author>.<name>` when the author is named,
/// so it can stay the same once published, otherwise `local.<name>`.
pub fn default_id(name: &str, author: &str) -> String {
    let author = package::slug(author);
    let name = package::slug(name);
    if author == "my-theme" {
        format!("local.{name}")
    } else {
        format!("io.github.{author}.{name}")
    }
}

/// The new theme's `theme.json`: the template's, with its id, name, author
/// and description filled in, `source` left for the author to set, and one
/// variant.
pub fn manifest(new: &NewTheme) -> Result<String, String> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err("a theme needs a name".into());
    }
    if let Some(start) = &new.start
        && start.dark == new.light
    {
        return Err(format!(
            "{} is a {} theme; {}",
            start.from,
            if start.dark { "dark" } else { "light" },
            if start.dark {
                "leave out --light"
            } else {
                "add --light"
            }
        ));
    }
    let template =
        std::str::from_utf8(template_file(MANIFEST).ok_or("the template has no theme.json")?)
            .map_err(|_| "the template's theme.json isn't text")?;
    let parsed: Value = serde_json::from_str(template)
        .map_err(|error| format!("the template's theme.json doesn't read: {error}"))?;
    let id = new
        .id
        .clone()
        .unwrap_or_else(|| default_id(name, &new.author));
    let author = new.author.trim();
    let quote = |text: &str| Value::String(text.to_owned()).to_string();
    let description = match &new.start {
        Some(start) => format!("{name}, from {}'s colours.", start.from),
        None => format!("{name}, a theme for Yapfast."),
    };
    // A checkout that turns line ends into CRLF (Git's `core.autocrlf` on
    // Windows) builds the template in with them; a new theme is written with
    // `\n`, as the rest of this does.
    let mut text = template.replace("\r\n", "\n");
    for (field, value) in [
        ("id", quote(&id)),
        ("name", quote(name)),
        ("description", quote(&description)),
        (
            "authors",
            if author.is_empty() {
                "[]".to_owned()
            } else {
                format!("[{}]", quote(author))
            },
        ),
        ("source", quote("")),
    ] {
        let was = parsed
            .get(field)
            .ok_or_else(|| format!("the template's theme.json has no \"{field}\""))?;
        let line = format!("\"{field}\": {was}");
        if !text.contains(&line) {
            return Err(format!(
                "the template's theme.json doesn't write \"{field}\" on one line"
            ));
        }
        text = text.replacen(&line, &format!("\"{field}\": {value}"), 1);
    }
    let mode = if new.light { Mode::Light } else { Mode::Dark };
    let variant = match &new.start {
        Some(start) => serde_json::json!({ "base": start.base, "colours": start.colours }),
        None => parsed["variants"][mode.name()].clone(),
    };
    if variant.is_null() {
        return Err(format!("the template has no {} variant", mode.name()));
    }
    // `variants` is the template's last field: everything from it is
    // written again with the one variant.
    let cut = text
        .find("\n  \"variants\":")
        .ok_or("the template's theme.json doesn't end with its variants")?;
    text.truncate(cut);
    let variants = serde_json::to_string_pretty(&serde_json::json!({ mode.name(): variant }))
        .expect("JSON values serialise")
        .replace('\n', "\n  ");
    let _ = write!(text, "\n  \"variants\": {variants}\n}}\n");
    let package = Package::parse(&text).map_err(|error| error.0)?;
    if package.id != id {
        return Err(format!("\"{id}\" isn't an id a theme can have"));
    }
    Ok(text)
}

/// Writes the new theme into `folder`, which has to be missing or empty: its
/// `theme.json`, the template's files it names and a licence to fill in.
/// Returns the files written, by their path in the folder.
pub fn create(folder: &Path, new: &NewTheme) -> Result<Vec<PathBuf>, String> {
    let text = manifest(new)?;
    if std::fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!("{} already holds files", folder.display()));
    }
    let package = Package::parse(&text).map_err(|error| error.0)?;
    let mut written = vec![PathBuf::from(MANIFEST)];
    let mut files = vec![(PathBuf::from("LICENSE"), template_file("LICENSE"))];
    files.extend(package.files().into_iter().map(|path| {
        let bytes = template_file(&super::theme_path(&path));
        (path, bytes)
    }));
    let write = |relative: &Path, bytes: &[u8]| -> Result<(), String> {
        let path = folder.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{} couldn't be made: {error}", parent.display()))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| format!("{} couldn't be written: {error}", path.display()))
    };
    write(Path::new(MANIFEST), text.as_bytes())?;
    for (relative, bytes) in files {
        if written.contains(&relative) {
            continue;
        }
        let bytes = match bytes {
            Some(bytes) => bytes,
            None if relative == Path::new("LICENSE") => LICENCE_PLACEHOLDER.as_bytes(),
            None => {
                return Err(format!(
                    "the template names {}, which it doesn't hold",
                    super::theme_path(&relative)
                ));
            }
        };
        write(&relative, bytes)?;
        written.push(relative);
    }
    Ok(written)
}

/// What `yapfast theme new` prints once the folder is made.
pub fn next_steps(folder: &Path, written: &[PathBuf], report: &check::Report) -> String {
    let shown = folder.display();
    let mut out = format!(
        "Made {shown} ({} files) from Yapfast's template.\n",
        written.len()
    );
    out.push_str(&report.text());
    let _ = write!(
        out,
        "\nNext:\n  \
         yapfast --theme-dev {shown}            # shows it, and again on each save\n  \
         yapfast theme check {shown}            # the checks an install runs\n  \
         yapfast theme check {shown} --marketplace   # and those for listing\n\n\
         Edit theme.json: the template sets every layer, so delete what you don't want.\n\
         Before you publish, set \"source\" to its GitHub repository, add a preview, and\n\
         put your licence in LICENSE and \"license\".\n"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::check::{Files, Strictness};

    fn new(name: &str) -> NewTheme {
        NewTheme {
            name: name.into(),
            author: "Ada Lovelace".into(),
            ..NewTheme::default()
        }
    }

    #[test]
    fn the_template_is_built_in_whole() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/template");
        let mut on_disk = Vec::new();
        let mut folders = vec![folder.clone()];
        while let Some(at) = folders.pop() {
            for entry in std::fs::read_dir(at).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    let relative = path.strip_prefix(&folder).unwrap();
                    on_disk.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        on_disk.sort();
        let built_in: Vec<String> = TEMPLATE.iter().map(|(path, _)| path.to_string()).collect();
        assert_eq!(built_in, on_disk);
    }

    #[test]
    fn a_new_theme_is_the_template_with_its_names_and_one_variant() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("tide");
        let written = create(&folder, &new("Tide")).unwrap();
        let files = Files::read(&folder).unwrap();
        let report = check::check(&files, Strictness::Install);
        assert!(report.passed(), "{}", report.text());
        let package = report.package.unwrap();
        assert_eq!(package.id, "io.github.ada-lovelace.tide");
        assert_eq!(package.name, "Tide");
        assert_eq!(package.authors, ["Ada Lovelace"]);
        assert_eq!(package.modes(), [Mode::Dark]);
        assert_eq!(package.resolve(Mode::Dark).unwrap().base, "yap");
        // Its files, and only those it names.
        for path in package.files() {
            assert!(folder.join(&path).is_file(), "{}", path.display());
        }
        assert!(written.contains(&PathBuf::from("LICENSE")));
        assert!(written.contains(&PathBuf::from("icons/send.svg")));
        assert!(!written.iter().any(|path| path.starts_with("cursors")));
        // The template's own layout survives: its blank lines and the order
        // of its fields.
        let text = std::fs::read_to_string(folder.join(MANIFEST)).unwrap();
        assert!(
            text.starts_with("{\n  \"theme_api\": 1,\n  \"id\": \"io.github.ada-lovelace.tide\"")
        );
        assert!(text.contains("\"source\": \"\""));
    }

    #[test]
    fn light_takes_the_template_s_light_variant() {
        let directory = tempfile::tempdir().unwrap();
        let mut light = new("Paper Moon");
        light.light = true;
        light.author = String::new();
        create(&directory.path().join("paper"), &light).unwrap();
        let package = Package::parse(
            &std::fs::read_to_string(directory.path().join("paper/theme.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(package.id, "local.paper-moon");
        assert!(package.authors.is_empty());
        assert_eq!(package.modes(), [Mode::Light]);
        assert_eq!(package.resolve(Mode::Light).unwrap().base, "sakura");
    }

    #[test]
    fn a_theme_can_start_from_another_s_colours() {
        let mut colours = serde_json::Map::new();
        colours.insert("chat".into(), "#2a1a3a".into());
        colours.insert("accent".into(), "linear(90deg, #ff9e64, #ff71ce)".into());
        let start = Start {
            from: "Plum".into(),
            base: "dracula".into(),
            dark: true,
            colours,
        };
        let mut from = new("Plum Two");
        from.start = Some(start);
        let text = manifest(&from).unwrap();
        let resolved = Package::parse(&text).unwrap().resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.base, "dracula");
        assert_eq!(
            resolved.paint("chat"),
            Some(&package::Paint::Colour(0xff2a_1a3a))
        );
        assert!(
            resolved.paint("on-accent").is_none(),
            "the template's colours go"
        );
        from.light = true;
        let error = manifest(&from).unwrap_err();
        assert!(error.contains("dark theme"), "{error}");
    }

    #[test]
    fn a_folder_with_files_is_left_alone() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("notes.txt"), "mine").unwrap();
        let error = create(directory.path(), &new("Tide")).unwrap_err();
        assert!(error.contains("already holds"), "{error}");
        assert!(manifest(&new("  ")).is_err());
        let mut odd = new("Tide");
        odd.id = Some("Not An Id".into());
        assert!(manifest(&odd).is_err());
    }
}
