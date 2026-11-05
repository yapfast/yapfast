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

/// Hears the microphone and plays on the speakers the way a call does, and says
/// what it found. The check to run when the sound is wrong: these are the same
/// streams a call reads and writes, so what it reports is what a call sends.
pub fn audio_test(seconds: u64) -> Result<()> {
    let seconds = seconds.clamp(1, 30);
    let audio = livekit::PlatformAudio::new().map_err(|_| {
        anyhow::anyhow!("audio devices unavailable; check audio service and permissions")
    })?;
    let microphone = audio
        .recording_devices()
        .next()
        .map(|device| device.name.clone())
        .unwrap_or_default();
    let speakers = audio
        .playout_devices()
        .next()
        .map(|device| device.name.clone())
        .unwrap_or_default();
    println!("Microphone: {microphone}\nSpeakers:   {speakers}");

    let rate = crate::audio_io::RATE;
    let mut input = crate::audio_io::Input::<f32>::open(
        None,
        "yapfast audio test",
        rate,
        2,
        std::time::Duration::from_millis(10),
    )?;
    let mut samples = vec![0.0f32; rate as usize / 20 * 2];
    let mut peak = 0.0f32;
    let mut energy = 0.0f64;
    let mut counted = 0usize;
    let mut broken = 0usize;
    for _ in 0..seconds as usize * 20 {
        input.read(&mut samples)?;
        for sample in &samples {
            // Samples that aren't numbers are the sound of a broken capture
            // path, and they reach the other person as noise.
            if !sample.is_finite() {
                broken += 1;
                continue;
            }
            peak = peak.max(sample.abs());
            energy += f64::from(*sample) * f64::from(*sample);
            counted += 1;
        }
    }
    let rms = (energy / counted.max(1) as f64).sqrt() as f32;
    println!("Heard for {seconds} s: peak {peak:.4}, rms {rms:.4}.");
    if broken > 0 {
        anyhow::bail!(
            "{broken} of {} samples were not sound at all, so the microphone path is broken and \
             nothing downstream can fix it",
            counted + broken
        );
    }
    if peak < 0.001 {
        println!(
            "Nothing was heard. Check the microphone isn't muted, and that Yapfast may use it."
        );
    } else if peak > 0.99 {
        println!("That clipped. Turn the input down, or move back from the microphone.");
    } else {
        println!("The microphone gives clean samples.");
    }

    let mut output = crate::audio_io::Output::<f32>::open_or_default(
        None,
        "yapfast audio test",
        rate,
        2,
        Some(std::time::Duration::from_millis(200)),
    )?;
    // Two notes rather than one, so it's obvious the sound came and went rather
    // than hummed, which is what a stuck stream sounds like.
    for frequency in [440.0f32, 660.0] {
        let mut tone = vec![0.0f32; rate as usize / 3 * 2];
        for (at, frame) in tone.chunks_exact_mut(2).enumerate() {
            let value =
                0.2 * (2.0 * std::f32::consts::PI * frequency * at as f32 / rate as f32).sin();
            frame[0] = value;
            frame[1] = value;
        }
        output.write(&tone)?;
    }
    output.drain()?;
    println!("Played a two-note chime. If you didn't hear it, choose other speakers in Settings.");
    Ok(())
}

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
