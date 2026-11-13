//! `yapfast theme new` and `yapfast theme export`, beside `theme check`: a
//! theme's folder made from Yapfast's template, or from one of the person's
//! own colour themes, with no window. See docs/themes.md, "Making a theme".

use std::path::{Path, PathBuf};

use yapfast_core::themes::Mode;
use yapfast_core::themes::check::{self, Files, Strictness};
use yapfast_core::themes::install::Themes;
use yapfast_core::themes::package::{self, LOCKED_COLOURS, Paint};
use yapfast_core::themes::scaffold::{self, NewTheme, Start};

use super::{Layout, theme, theme_look};

const NEW_USAGE: &str = "usage: yapfast theme new <name> [--light] [--from <theme>] [--author <name>] [--id <id>] [--dir <folder>] [--data-dir <folder>]";
const EXPORT_USAGE: &str =
    "usage: yapfast theme export <custom theme> <folder> [--author <name>] [--data-dir <folder>]";

/// Takes `flag` and the value after it out of `arguments`.
fn value(arguments: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    let Some(at) = arguments.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    arguments.remove(at);
    if at >= arguments.len() {
        return Err(format!("{flag} needs a value"));
    }
    Ok(Some(arguments.remove(at)))
}

/// Takes `flag` out of `arguments`, saying whether it was there.
fn flag(arguments: &mut Vec<String>, flag: &str) -> bool {
    let before = arguments.len();
    arguments.retain(|argument| argument != flag);
    arguments.len() != before
}

/// Where Yapfast keeps its data: `--data-dir`'s folder, or the default.
fn data_dir(arguments: &mut Vec<String>) -> Result<PathBuf, String> {
    match value(arguments, "--data-dir")? {
        Some(folder) => Ok(folder.into()),
        None => yapfast_core::default_directory().map_err(|error| error.to_string()),
    }
}

/// The person's name for a theme they make here: `--author`, or their
/// account's name on this computer.
fn author(arguments: &mut Vec<String>) -> Result<String, String> {
    Ok(value(arguments, "--author")?
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .unwrap_or_default())
}

/// The one positional argument left, refusing an option this doesn't know.
fn positional(arguments: &[String], usage: &str) -> Result<Vec<String>, String> {
    if let Some(unknown) = arguments.iter().find(|argument| argument.starts_with("--")) {
        return Err(format!("{unknown} isn't an option here\n{usage}"));
    }
    Ok(arguments.to_vec())
}

/// Every colour of one of Appearance's themes, by its name in
/// `ui/theme.slint`, as ARGB; never the protected ones.
fn colours_of(shown: &theme::Theme) -> Vec<(String, u32)> {
    theme::Theme::NAMES
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            let token = token.replace('_', "-");
            if LOCKED_COLOURS.contains(&token.as_str()) {
                return None;
            }
            Some((token, shown.colour(index)?.as_argb_encoded()))
        })
        .collect()
}

/// A colour or gradient as `theme.json` writes it.
fn paint_text(paint: &Paint) -> String {
    match paint {
        Paint::Colour(colour) => package::hex(*colour),
        Paint::Linear { angle, stops } => format!(
            "linear({angle}deg, {})",
            stops
                .iter()
                .map(|(colour, at)| format!("{} {}%", package::hex(*colour), at * 100.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Whether `wanted` names a theme, by its id or its name in any case.
fn names(wanted: &str, id: &str, name: &str) -> bool {
    wanted == id || wanted.eq_ignore_ascii_case(name)
}

/// The theme `--from` names: one Yapfast comes with, one of the person's own
/// (`custom:2`, or its name), or an installed theme, in its light variant
/// when `light` asks for it.
fn start_from(wanted: &str, data_dir: &Path, light: Option<bool>) -> Result<Start, String> {
    let wanted = wanted.trim();
    if let Some(built_in) = theme::BUILT_IN
        .iter()
        .find(|built_in| names(wanted, built_in.id, built_in.name))
    {
        return Ok(Start {
            from: built_in.name.to_owned(),
            base: built_in.id.to_owned(),
            dark: built_in.theme.dark,
            colours: serde_json::Map::new(),
        });
    }
    let layout = Layout::read(data_dir);
    if let Some(custom) = layout
        .custom_themes
        .iter()
        .find(|custom| names(wanted, &custom.id, &custom.name))
    {
        return Ok(Start {
            from: custom.name.clone(),
            base: custom.base.to_owned(),
            dark: custom.theme.dark,
            colours: colours_of(&custom.theme)
                .into_iter()
                .map(|(token, colour)| (token, package::hex(colour).into()))
                .collect(),
        });
    }
    let installed = Themes::open(data_dir).installed();
    if let Some(installed) = installed
        .iter()
        .find(|installed| names(wanted, &installed.id, &installed.package.name))
    {
        let package = &installed.package;
        let modes = package.modes();
        let mode = match light {
            Some(true) => Mode::Light,
            Some(false) => Mode::Dark,
            None => modes.first().copied().unwrap_or(Mode::Dark),
        };
        if !modes.contains(&mode) {
            return Err(format!("{} has no {} variant", package.name, mode.name()));
        }
        let resolved = package
            .resolve(mode)
            .map_err(|error| format!("{} doesn't read: {error}", package.name))?;
        return Ok(Start {
            from: package.name.clone(),
            base: resolved.base.clone(),
            dark: resolved.dark,
            colours: resolved
                .colours
                .iter()
                .map(|(token, paint)| ((*token).to_owned(), paint_text(paint).into()))
                .collect(),
        });
    }
    let mut known: Vec<String> = theme::BUILT_IN
        .iter()
        .map(|built_in| built_in.id.to_owned())
        .collect();
    known.extend(
        layout
            .custom_themes
            .iter()
            .map(|custom| format!("{} ({})", custom.id, custom.name)),
    );
    known.extend(installed.iter().map(|installed| installed.id.clone()));
    Err(format!(
        "no theme is called \"{wanted}\": --from takes one of {}",
        known.join(", ")
    ))
}

/// Checks a folder as an install would, printing what it found.
fn check_folder(folder: &Path) -> Result<check::Report, String> {
    theme_look::register_bases();
    let files = Files::read(folder)?;
    Ok(check::check(&files, Strictness::Install))
}

/// `yapfast theme new <name>`: prints the next steps, and returns the exit
/// status.
pub fn run_new(arguments: &[String]) -> i32 {
    match new(arguments.to_vec()) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn new(mut arguments: Vec<String>) -> Result<String, String> {
    let light = flag(&mut arguments, "--light");
    let from = value(&mut arguments, "--from")?;
    let id = value(&mut arguments, "--id")?;
    let parent = value(&mut arguments, "--dir")?.map_or_else(|| PathBuf::from("."), PathBuf::from);
    let author = author(&mut arguments)?;
    let data_dir = value(&mut arguments, "--data-dir")?;
    let rest = positional(&arguments, NEW_USAGE)?;
    let [name] = rest.as_slice() else {
        return Err(NEW_USAGE.into());
    };
    let start = match &from {
        Some(from) => {
            let data_dir = match data_dir {
                Some(folder) => PathBuf::from(folder),
                None => yapfast_core::default_directory().map_err(|error| error.to_string())?,
            };
            Some(start_from(from, &data_dir, light.then_some(true))?)
        }
        None => None,
    };
    let new = NewTheme {
        name: name.clone(),
        id,
        author,
        light: light || start.as_ref().is_some_and(|start| !start.dark),
        start,
    };
    let folder = parent.join(package::slug(name));
    let written = scaffold::create(&folder, &new)?;
    let report = check_folder(&folder)?;
    Ok(scaffold::next_steps(&folder, &written, &report))
}

/// `yapfast theme export <custom theme> <folder>`: one of the person's own
/// colour themes as a package, as Appearance's Export writes it.
pub fn run_export(arguments: &[String]) -> i32 {
    match export(arguments.to_vec()) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn export(mut arguments: Vec<String>) -> Result<String, String> {
    let author = author(&mut arguments)?;
    let data_dir = data_dir(&mut arguments)?;
    let rest = positional(&arguments, EXPORT_USAGE)?;
    let [wanted, folder] = rest.as_slice() else {
        return Err(EXPORT_USAGE.into());
    };
    let layout = Layout::read(&data_dir);
    let Some(custom) = layout
        .custom_themes
        .iter()
        .find(|custom| names(wanted, &custom.id, &custom.name))
    else {
        let mine: Vec<String> = layout
            .custom_themes
            .iter()
            .map(|custom| format!("{} ({})", custom.id, custom.name))
            .collect();
        return Err(if mine.is_empty() {
            format!(
                "there are no custom themes in {}: make one with New theme in Settings → Appearance",
                data_dir.display()
            )
        } else {
            format!(
                "no custom theme is called \"{wanted}\": {}",
                mine.join(", ")
            )
        });
    };
    let folder = PathBuf::from(folder);
    if std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!("{} already holds files", folder.display()));
    }
    let written = write_custom(
        &folder,
        &custom.name,
        &author,
        custom.base,
        &custom.theme,
        custom.folder.as_deref(),
        &data_dir,
    )?;
    let report = check_folder(&folder)?;
    let mut out = format!(
        "Exported {} to {} ({} files).\n{}",
        custom.name,
        folder.display(),
        written.len(),
        report.text()
    );
    out.push_str(
        "Put it in a GitHub repository with a preview, and it installs with Add from URL in Settings → Themes.\n",
    );
    Ok(out)
}

/// A custom theme's own folder, with its other layers and their files:
/// relative to the data folder, or absolute.
fn own_folder(folder: Option<&str>, data_dir: &Path) -> Option<PathBuf> {
    folder.map(|folder| data_dir.join(folder))
}

/// Writes a colour theme as a package into `dest`: every colour over its
/// base, and with `folder` (a custom theme's own folder, see
/// [`theme::Custom::folder`]) its other layers and every file they name,
/// fonts, pictures and sounds, with their licences. Appearance's Export and
/// `yapfast theme export` both write through here.
pub(super) fn write_custom(
    dest: &Path,
    name: &str,
    author: &str,
    base: &str,
    shown: &theme::Theme,
    folder: Option<&str>,
    data_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let source = own_folder(folder, data_dir);
    let layers = match &source {
        Some(source) => {
            match std::fs::read_to_string(source.join(yapfast_core::themes::MANIFEST)) {
                Ok(text) => Some(text),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(format!("{}: {error}", source.display())),
            }
        }
        None => None,
    };
    let colours = colours_of(shown);
    let named: Vec<(&str, u32)> = colours
        .iter()
        .map(|(token, colour)| (token.as_str(), *colour))
        .collect();
    let text = package::export_with(name, author, base, shown.dark, &named, layers.as_deref())?;
    package::write_export(dest, &text, source.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_core::themes::Package;
    use yapfast_core::themes::install::Fetched;

    fn run(
        arguments: &[&str],
        then: fn(Vec<String>) -> Result<String, String>,
    ) -> Result<String, String> {
        then(
            arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
        )
    }

    const FONT: &[u8] = include_bytes!("../../ui/fonts/DisplayName-Journal.ttf");
    const PICTURE: &[u8] = include_bytes!("../../themes/template/preview.png");

    /// A data folder with one custom theme, as Appearance saves it, with a
    /// folder of its own: an imported font and a background picture.
    fn data_with_plum() -> tempfile::TempDir {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join("layout.json"),
            r##"{ "custom_themes": [ { "id": "custom:1", "name": "Plum", "base": "yap", "dark": true,
                "colours": { "chat": "#2a1a3a", "sidebar": "#221430", "accent": "#ff9e64" },
                "folder": "custom-themes/1" } ] }"##,
        )
        .unwrap();
        let own = data.path().join("custom-themes/1");
        std::fs::create_dir_all(own.join("fonts")).unwrap();
        std::fs::create_dir_all(own.join("art")).unwrap();
        std::fs::write(own.join("fonts/Journal.ttf"), FONT).unwrap();
        std::fs::write(own.join("fonts/OFL.txt"), "SIL Open Font License 1.1").unwrap();
        std::fs::write(own.join("art/sky.png"), PICTURE).unwrap();
        std::fs::write(
            own.join("theme.json"),
            r#"{ "type": { "names": { "font": "fonts/Journal.ttf" } },
                 "art": { "surfaces": { "window": { "image": "art/sky.png", "opacity": 0.6 } } } }"#,
        )
        .unwrap();
        data
    }

    fn read_back(folder: &Path) -> Package {
        Package::parse(
            &std::fs::read_to_string(folder.join(yapfast_core::themes::MANIFEST)).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn new_makes_a_checked_folder_and_says_what_next() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().to_str().unwrap();
        let out = run(&["Tide Pool", "--author", "Ada", "--dir", dir], new).unwrap();
        let folder = parent.path().join("tide-pool");
        assert!(
            out.contains(&format!("yapfast --theme-dev {}", folder.display())),
            "{out}"
        );
        assert!(out.contains("yapfast theme check"), "{out}");
        let package = read_back(&folder);
        assert_eq!(package.id, "io.github.ada.tide-pool");
        assert_eq!(package.name, "Tide Pool");
        assert_eq!(package.authors, ["Ada"]);
        assert_eq!(package.modes(), [Mode::Dark]);
        let report = check_folder(&folder).unwrap();
        assert!(report.passed(), "{}", report.text());

        let out = run(&["Tide Pool", "--dir", dir], new).unwrap_err();
        assert!(out.contains("already holds"), "{out}");
        let out = run(&["Tide", "--colour", "x", "--dir", dir], new).unwrap_err();
        assert!(out.contains("--colour"), "{out}");
    }

    #[test]
    fn new_starts_from_a_built_in_or_a_custom_theme() {
        let data = data_with_plum();
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().to_str().unwrap();
        let data_dir = data.path().to_str().unwrap();
        run(
            &[
                "Night",
                "--from",
                "Dracula",
                "--dir",
                dir,
                "--data-dir",
                data_dir,
            ],
            new,
        )
        .unwrap();
        let resolved = read_back(&parent.path().join("night"))
            .resolve(Mode::Dark)
            .unwrap();
        assert_eq!(resolved.base, "dracula");
        assert!(resolved.colours.is_empty(), "the base's own colours");

        run(
            &[
                "Dawn",
                "--from",
                "sakura",
                "--dir",
                dir,
                "--data-dir",
                data_dir,
            ],
            new,
        )
        .unwrap();
        let package = read_back(&parent.path().join("dawn"));
        assert_eq!(package.modes(), [Mode::Light], "a light theme starts light");

        run(
            &[
                "Plum Two",
                "--from",
                "plum",
                "--dir",
                dir,
                "--data-dir",
                data_dir,
            ],
            new,
        )
        .unwrap();
        let resolved = read_back(&parent.path().join("plum-two"))
            .resolve(Mode::Dark)
            .unwrap();
        assert_eq!(resolved.base, "yap");
        assert_eq!(resolved.paint("chat"), Some(&Paint::Colour(0xff2a_1a3a)));

        let error = run(
            &[
                "X",
                "--light",
                "--from",
                "nord",
                "--dir",
                dir,
                "--data-dir",
                data_dir,
            ],
            new,
        )
        .unwrap_err();
        assert!(error.contains("dark theme"), "{error}");
        let error = run(
            &["X", "--from", "nope", "--dir", dir, "--data-dir", data_dir],
            new,
        )
        .unwrap_err();
        assert!(error.contains("custom:1 (Plum)"), "{error}");
    }

    #[test]
    fn an_exported_custom_theme_installs_with_the_same_colours() {
        let data = data_with_plum();
        let out_dir = tempfile::tempdir().unwrap();
        let folder = out_dir.path().join("plum");
        let out = run(
            &[
                "custom:1",
                folder.to_str().unwrap(),
                "--author",
                "Ada",
                "--data-dir",
                data.path().to_str().unwrap(),
            ],
            export,
        )
        .unwrap();
        assert!(out.contains("passed"), "{out}");
        let package = read_back(&folder);
        assert_eq!(package.id, "local.plum");
        assert_eq!(package.authors, ["Ada"]);
        assert_eq!(package.version, "1.0.0");
        assert!(folder.join("LICENSE").is_file());

        // Installed as Add from URL installs it, once it's in a repository.
        register_for_test();
        let fetched = Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            "0123456789abcdef".into(),
            Files::read(&folder).unwrap(),
        )
        .unwrap();
        let installed_in = tempfile::tempdir().unwrap();
        let store = Themes::open(installed_in.path());
        store.install(&fetched).unwrap();
        let installed = store.installed();
        let resolved = installed[0].package.resolve(Mode::Dark).unwrap();
        let custom = Layout::read(data.path()).custom_themes.remove(0);
        assert_eq!(
            custom.folder.as_deref(),
            Some("custom-themes/1"),
            "kept as saved"
        );
        assert_eq!(resolved.base, custom.base);
        // Its font and picture travel with it, byte for byte.
        let names = resolved.types[&yapfast_core::themes::package::Role::Names]
            .font
            .clone()
            .unwrap();
        assert_eq!(
            std::fs::read(installed[0].folder.join(names)).unwrap(),
            FONT
        );
        assert_eq!(
            std::fs::read(installed[0].folder.join("art/sky.png")).unwrap(),
            PICTURE
        );
        assert!(
            folder.join("fonts/OFL.txt").is_file(),
            "the font's licence goes along"
        );
        for (token, colour) in colours_of(&custom.theme) {
            assert_eq!(
                resolved.paint(&token),
                Some(&Paint::Colour(colour)),
                "{token} installs as it was made"
            );
        }

        let error = run(
            &[
                "Nope",
                "elsewhere",
                "--data-dir",
                data.path().to_str().unwrap(),
            ],
            export,
        )
        .unwrap_err();
        assert!(error.contains("custom:1 (Plum)"), "{error}");
    }

    fn register_for_test() {
        theme_look::register_bases();
    }
}
