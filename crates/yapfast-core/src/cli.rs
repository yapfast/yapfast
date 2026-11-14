#[cfg(feature = "matrix")]
use std::{
    collections::BTreeMap,
    io::{self, BufRead, IsTerminal},
    path::PathBuf,
};

#[cfg(feature = "matrix")]
use crate::session::Session;
use anyhow::Result;
#[cfg(feature = "matrix")]
use anyhow::{Context, bail};

/// List native audio devices without creating or publishing a microphone track.
pub fn audio_devices() -> Result<()> {
    let audio = livekit::PlatformAudio::new().map_err(|_| {
        anyhow::anyhow!("audio devices unavailable; check audio service and permissions")
    })?;
    println!("Microphones:");
    for device in audio.recording_devices() {
        println!("{}", device.name);
    }
    println!("Speakers:");
    for device in audio.playout_devices() {
        println!("{}", device.name);
    }
    println!("Cameras:");
    for device in crate::camera::list() {
        println!("{}", device.name);
    }
    println!("No microphone track was created or published, and no camera filmed.");
    Ok(())
}

/// The argument that keeps the plugin system off for this run.
#[cfg(feature = "plugins")]
pub const NO_PLUGINS: &str = "--no-plugins";

/// What `--help` says about [`NO_PLUGINS`], in the manner of the other flags.
#[cfg(feature = "plugins")]
pub const NO_PLUGINS_HELP: &str =
    "--no-plugins starts without the plugin system, even where plugins are enabled.";

/// Whether this run asked for no plugins, from [`parse_no_plugins`].
#[cfg(feature = "plugins")]
static ASKED_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Takes `--no-plugins` out of the arguments Yapfast was started with, and
/// records it: a flag read before anything else is opened can't be left for the
/// window to find, since the window isn't there yet.
///
/// The application asks [`plugins_allowed`] once, at startup, and opens no
/// plugin state when this run asked for none.
#[cfg(feature = "plugins")]
pub fn parse_no_plugins(arguments: &mut Vec<String>) -> bool {
    let asked = arguments.iter().any(|argument| argument == NO_PLUGINS);
    arguments.retain(|argument| argument != NO_PLUGINS);
    ASKED_OFF.store(asked, std::sync::atomic::Ordering::Relaxed);
    asked
}

/// Whether a plugin may run in this process at all: false when this run was
/// started with [`NO_PLUGINS`], and true otherwise, whether or not any plugin
/// is enabled.
#[cfg(feature = "plugins")]
pub fn plugins_allowed() -> bool {
    !ASKED_OFF.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "matrix")]
pub async fn session(arguments: &[String]) -> Result<()> {
    let (command, arguments) = arguments
        .split_first()
        .context("choose a session command; run yapfast --help")?;
    let mut options = BTreeMap::new();
    let mut flags = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(option) = arguments.next() {
        match option.as_str() {
            "--password-stdin" | "--key-stdin" => {
                if flags.contains(option) {
                    bail!("duplicate session option")
                }
                flags.push(option.clone());
            }
            "--data-dir" | "--homeserver" | "--user" => {
                let value = arguments
                    .next()
                    .context("session option is missing its value")?;
                if value.starts_with("--")
                    || options.insert(option.clone(), value.clone()).is_some()
                {
                    bail!("invalid or duplicate session option")
                }
            }
            _ => bail!("unknown session option; run yapfast --help"),
        }
    }
    let root = match options.remove("--data-dir") {
        Some(path) => PathBuf::from(path),
        None => crate::default_directory()?,
    };
    match command.as_str() {
        "login" => {
            let homeserver = options
                .remove("--homeserver")
                .context("sign-in needs --homeserver")?;
            let user = options.remove("--user").context("sign-in needs --user")?;
            if !options.is_empty() || flags != ["--password-stdin"] {
                bail!("sign-in needs --password-stdin; run yapfast --help")
            }
            let password = read_secret()?;
            let session = Session::login(root, &homeserver, &user, &password).await?;
            session.synchronize().await?;
            status(&session).await
        }
        "status" | "logout" | "recover" => {
            let expected: Vec<String> = if command == "recover" {
                vec!["--key-stdin".into()]
            } else {
                vec![]
            };
            if !options.is_empty() || flags != expected {
                bail!("invalid options for this session command; run yapfast --help")
            }
            let session = Session::restore(root).await?;
            match command.as_str() {
                "logout" => {
                    session.logout().await?;
                    println!("Signed out. Encrypted SDK stores were retained.");
                    Ok(())
                }
                "recover" => {
                    session.recover(&read_secret()?).await?;
                    status(&session).await
                }
                _ => {
                    session.synchronize().await?;
                    status(&session).await
                }
            }
        }
        _ => bail!("unknown session command; run yapfast --help"),
    }
}

#[cfg(feature = "matrix")]
fn read_secret() -> Result<String> {
    if io::stdin().is_terminal() {
        bail!(
            "provide the secret through stdin from a hidden prompt or password manager; do not put it in command arguments"
        )
    }
    let mut secret = String::new();
    io::stdin()
        .lock()
        .read_line(&mut secret)
        .context("could not read the secret from stdin")?;
    while secret.ends_with(['\r', '\n']) {
        secret.pop();
    }
    if secret.is_empty() {
        bail!("stdin contained no secret")
    }
    Ok(secret)
}

#[cfg(feature = "matrix")]
async fn status(session: &Session) -> Result<()> {
    println!(
        "Account: {}",
        session.client.user_id().context("not signed in")?
    );
    println!(
        "Device: {}",
        session.client.device_id().context("not signed in")?
    );
    println!(
        "Device trust: {}",
        if session.device_is_trusted().await? {
            "cross-signed and verified"
        } else {
            "recovery or device verification required"
        }
    );
    Ok(())
}
