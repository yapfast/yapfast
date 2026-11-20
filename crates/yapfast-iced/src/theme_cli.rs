//! `yapfast theme check | install | new | export`, with no window: a
//! theme's checks printed, a local folder installed, a theme's folder made
//! from the template or from another theme, and one of the person's own
//! colour themes written as a package. See docs/themes.md, "Making a theme".

use std::path::{Path, PathBuf};

use yapfast_core::{
    theme as palette,
    themes::{
        Mode,
        check::{self, Files, Strictness},
        install::{self, Fetched, Themes},
        package::{self, LOCKED_COLOURS, Paint},
        scaffold::{self, NewTheme, Start},
    },
};

use crate::layout::Layout;

pub const USAGE: &str = "usage: yapfast theme check <folder> [--marketplace]
       yapfast theme install <folder> [--data-dir <folder>]
       yapfast theme new <name> [--light] [--from <theme>] [--author <name>] [--id <id>] [--dir <folder>] [--data-dir <folder>]
       yapfast theme export <custom theme> <folder> [--author <name>] [--data-dir <folder>]";
const CHECK_USAGE: &str = "usage: yapfast theme check <folder> [--marketplace]";
const INSTALL_USAGE: &str = "usage: yapfast theme install <folder> [--data-dir <folder>]";
const NEW_USAGE: &str = "usage: yapfast theme new <name> [--light] [--from <theme>] [--author <name>] [--id <id>] [--dir <folder>] [--data-dir <folder>]";
const EXPORT_USAGE: &str =
    "usage: yapfast theme export <custom theme> <folder> [--author <name>] [--data-dir <folder>]";

/// Runs `yapfast theme …` (`arguments` after `theme`), printing what it did,
/// and returns the exit status: 0 done, 1 failed, 2 not understood.
pub fn run(arguments: &[String]) -> i32 {
    let (command, rest) = match arguments.split_first() {
        Some((command, rest)) => (command.as_str(), rest.to_vec()),
        None => ("", Vec::new()),
    };
    let done = match command {
        "check" => return check(rest),
        "install" => install(rest),
        "new" => new(rest),
        "export" => export(rest),
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    match done {
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

/// `theme check <folder> [--marketplace]`: the checks an install, and with
/// `--marketplace` a listing, run; on a pass, the digest the catalog pins.
fn check(mut arguments: Vec<String>) -> i32 {
    let marketplace = flag(&mut arguments, "--marketplace");
    let folder = match positional(&arguments, CHECK_USAGE)
        .and_then(|folders| <[String; 1]>::try_from(folders).map_err(|_| CHECK_USAGE.to_owned()))
    {
        Ok([folder]) => folder,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let files = match Files::read(Path::new(&folder)) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("error: {error}\nfailed");
            return 1;
        }
    };
    let strictness = if marketplace {
        Strictness::Marketplace
    } else {
        Strictness::Install
    };
    let report = check::check(&files, strictness);
    print!("{}", report.text());
    if report.passed() {
        println!("sha256 (for the catalog): {}", install::digest(&files));
    }
    i32::from(!report.passed())
}

/// `theme install <folder>`: a checked local folder into the theme store.
fn install(mut arguments: Vec<String>) -> Result<String, String> {
    let data = data_dir(&mut arguments)?;
    let [folder] = positional(&arguments, INSTALL_USAGE)?
        .try_into()
        .map_err(|_| INSTALL_USAGE.to_owned())?;
    let fetched = Fetched::local(Path::new(&folder))?;
    Themes::open(&data)
        .install(&fetched)
        .map_err(|error| format!("{error:#}"))?;
    Ok(format!(
        "Installed {}. Apply it in Settings → Themes.\n",
        fetched.package().id
    ))
}

/// `theme new <name>`: a theme's folder from the template, or from the
/// theme `--from` names, checked, with what to do next.
fn new(mut arguments: Vec<String>) -> Result<String, String> {
    let light = flag(&mut arguments, "--light");
    let from = value(&mut arguments, "--from")?;
    let id = value(&mut arguments, "--id")?;
    let parent = value(&mut arguments, "--dir")?.map_or_else(|| PathBuf::from("."), PathBuf::from);
    let author = author(&mut arguments)?;
    let data = value(&mut arguments, "--data-dir")?;
    let [name] = positional(&arguments, NEW_USAGE)?
        .try_into()
        .map_err(|_| NEW_USAGE.to_owned())?;
    let start = match &from {
        Some(from) => {
            let data = match data {
                Some(folder) => PathBuf::from(folder),
                None => yapfast_core::default_directory().map_err(|error| error.to_string())?,
            };
            Some(start_from(from, &data, light.then_some(true))?)
        }
        None => None,
    };
    let new = NewTheme {
        light: light || start.as_ref().is_some_and(|start| !start.dark),
        name: name.clone(),
        id,
        author,
        start,
    };
    let folder = parent.join(package::slug(&name));
    let written = scaffold::create(&folder, &new)?;
    let report = check::check(&Files::read(&folder)?, Strictness::Install);
    Ok(scaffold::next_steps(&folder, &written, &report))
}

/// `theme export <custom theme> <folder>`: one of the person's own themes
/// as a package, into a folder that's empty or not there. It never writes
/// over files already there.
fn export(mut arguments: Vec<String>) -> Result<String, String> {
    let author = author(&mut arguments)?;
    let data = data_dir(&mut arguments)?;
    let [wanted, folder] = positional(&arguments, EXPORT_USAGE)?
        .try_into()
        .map_err(|_| EXPORT_USAGE.to_owned())?;
    let folder = PathBuf::from(folder);
    if std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!(
            "{} already holds files; export into an empty or new folder",
            folder.display()
        ));
    }
    let layout = Layout::read(&data);
    let Some(custom) = layout
        .custom_themes
        .iter()
        .find(|custom| names(&wanted, &custom.id, &custom.name))
    else {
        let mine: Vec<String> = layout
            .custom_themes
            .iter()
            .map(|custom| format!("{} ({})", custom.id, custom.name))
            .collect();
        return Err(if mine.is_empty() {
            format!(
                "there are no themes of your own in {}: make one with New theme in Settings → Themes",
                data.display()
            )
        } else {
            format!(
                "none of your themes is called \"{wanted}\": {}",
                mine.join(", ")
            )
        });
    };
    let source = custom.folder.as_deref().map(|own| data.join(own));
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
    let colours = colours_of(&custom.theme);
    let named: Vec<(&str, u32)> = colours
        .iter()
        .map(|(token, colour)| (token.as_str(), *colour))
        .collect();
    let text = package::export_with(
        &custom.name,
        &author,
        custom.base,
        custom.theme.dark,
        &named,
        layers.as_deref(),
    )?;
    let written = package::write_export(&folder, &text, source.as_deref())?;
    let report = check::check(&Files::read(&folder)?, Strictness::Install);
    Ok(format!(
        "Exported {} to {} ({} files).\n{}Put it in a GitHub repository with a preview, and it installs with Add from URL in Settings → Themes.\n",
        custom.name,
        folder.display(),
        written.len(),
        report.text()
    ))
}

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

/// `--data-dir`'s folder, or where Yapfast keeps its data.
fn data_dir(arguments: &mut Vec<String>) -> Result<PathBuf, String> {
    match value(arguments, "--data-dir")? {
        Some(folder) => Ok(folder.into()),
        None => yapfast_core::default_directory().map_err(|error| error.to_string()),
    }
}

/// `--author`, or the account's name on this computer.
fn author(arguments: &mut Vec<String>) -> Result<String, String> {
    Ok(value(arguments, "--author")?
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .unwrap_or_default())
}

/// What's left once the options are taken, refusing an option this doesn't
/// know.
fn positional(arguments: &[String], usage: &str) -> Result<Vec<String>, String> {
    if let Some(unknown) = arguments.iter().find(|argument| argument.starts_with("--")) {
        return Err(format!("{unknown} isn't an option here\n{usage}"));
    }
    Ok(arguments.to_vec())
}

/// Whether `wanted` names a theme, by its id or its name in any case.
fn names(wanted: &str, id: &str, name: &str) -> bool {
    wanted == id || wanted.eq_ignore_ascii_case(name)
}

/// Every colour of a palette theme by its `theme.json` name, as ARGB; never
/// the protected ones.
fn colours_of(shown: &palette::Theme) -> Vec<(String, u32)> {
    palette::Theme::TOKENS
        .iter()
        .enumerate()
        .filter(|(_, token)| !LOCKED_COLOURS.contains(token))
        .filter_map(|(index, token)| Some(((*token).to_owned(), shown.colour(index)?.argb())))
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

/// The theme `--from` names: one Yapfast comes with, one of the person's own
/// (`custom:2`, or its name), or an installed one, in its light variant when
/// `light` asks for it.
fn start_from(wanted: &str, data: &Path, light: Option<bool>) -> Result<Start, String> {
    let wanted = wanted.trim();
    if let Some(built_in) = palette::BUILT_IN
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
    let layout = Layout::read(data);
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
    let installed = Themes::open(data).installed();
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
    let mut known: Vec<String> = palette::BUILT_IN
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect()
    }

    #[test]
    fn a_new_theme_is_made_checked_and_installs() {
        let folder = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let made = new(strings(&[
            "Night Tea",
            "--dir",
            folder.path().to_str().unwrap(),
            "--author",
            "Me",
        ]))
        .expect("the template makes a theme");
        assert!(made.contains("night-tea"), "{made}");
        let theme = folder.path().join("night-tea");
        assert_eq!(check(strings(&[theme.to_str().unwrap()])), 0);
        install(strings(&[
            theme.to_str().unwrap(),
            "--data-dir",
            data.path().to_str().unwrap(),
        ]))
        .expect("a checked theme installs");
        assert_eq!(Themes::open(data.path()).installed().len(), 1);
    }

    #[test]
    fn export_never_writes_into_a_folder_with_files() {
        let data = tempfile::tempdir().unwrap();
        let into = tempfile::tempdir().unwrap();
        std::fs::write(into.path().join("keep.txt"), "mine").unwrap();
        let error = export(strings(&[
            "anything",
            into.path().to_str().unwrap(),
            "--data-dir",
            data.path().to_str().unwrap(),
        ]))
        .unwrap_err();
        assert!(error.contains("already holds files"), "{error}");
        assert_eq!(
            std::fs::read_to_string(into.path().join("keep.txt")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn unknown_commands_and_options_are_refused() {
        assert_eq!(run(&strings(&["frobnicate"])), 2);
        assert_eq!(check(strings(&["--force", "x"])), 2);
        assert_eq!(check(strings(&["a", "b"])), 2);
        assert_eq!(check(strings(&[])), 2);
        assert!(install(strings(&["--force", "x"])).is_err());
    }
}
