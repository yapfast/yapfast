//! Developer probe for Discord voice, run by hand with a test account.
//! It is not part of Yapfast. See docs/discord/README.md#testing.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use yapfast_discord::{
    api::Api,
    ready::Access,
    service::{Discord, Snapshot, Status},
    token,
    voice::{CHANNELS, Call, FRAME, Listener, SAMPLE_RATE, live_microphone},
};

const USAGE: &str = "usage: yapfast-discord-probe <command>
  store-token              save the test account's token in the OS credential store
  whoami                   show which account the saved token belongs to
  servers                  list servers and their text and voice channels
  messages <channel>       print a text channel's latest messages
  join <channel> [seconds] join a voice channel, play beeps, record what others say
  forget-token             remove the saved token";
const RECORDINGS: &str = "target/discord-probe";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("store-token") => {
            let token = read_hidden("Paste the test account's token (hidden): ")?;
            token::store(&token)?;
            println!("Saved in the OS credential store.");
            whoami().await
        }
        Some("whoami") => whoami().await,
        Some("servers") => servers().await,
        Some("messages") => messages(args.get(1).context(USAGE)?).await,
        Some("join") => {
            let channel = args.get(1).context(USAGE)?;
            let seconds = match args.get(2) {
                Some(seconds) => seconds.parse().context("seconds must be a number")?,
                None => 60,
            };
            join(channel, seconds).await
        }
        Some("forget-token") => {
            token::forget()?;
            println!("Removed the token from the OS credential store.");
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

async fn whoami() -> Result<()> {
    let user = Api::new(token::load()?)?.me().await?;
    let name = user.global_name.as_deref().unwrap_or(&user.username);
    println!(
        "Signed in to Discord as {name} (@{}), id {}.",
        user.username, user.id
    );
    Ok(())
}

async fn servers() -> Result<()> {
    let discord = Discord::start(token::load()?)?;
    online(&discord).await?;
    // Voice states arrive just after connecting, in READY_SUPPLEMENTAL.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let snapshot = discord.snapshots().borrow().clone();
    for server in &snapshot.servers {
        println!("{} ({})", server.name, server.id);
        let text = server.text_channels().map(|channel| ("text", channel));
        let voice = server.voice_channels().map(|channel| ("voice", channel));
        for (kind, channel) in text.chain(voice) {
            let marks = channel.marks;
            let access = match marks.access {
                Access::Join => "",
                Access::Locked if kind == "text" => ", read only",
                Access::Locked => ", locked",
                Access::Hidden => ", hidden",
            };
            let unread = match (marks.unread, marks.mentions, marks.muted) {
                (_, mentions, _) if mentions > 0 => format!(", {mentions} mentions"),
                (true, ..) => ", unread".into(),
                (_, _, true) => ", muted".into(),
                _ => String::new(),
            };
            println!(
                "  {kind}: {} ({}{access}{unread})",
                channel.name, channel.id
            );
            for member in snapshot.voice.get(&channel.id).into_iter().flatten() {
                println!("    in voice: {}", member.name);
            }
        }
    }
    Ok(())
}

/// Prints the latest page of a text channel as Yapfast would show it. It reads only.
async fn messages(channel_id: &str) -> Result<()> {
    let discord = Discord::start(token::load()?)?;
    let snapshot = online(&discord).await?;
    let server = snapshot
        .servers
        .iter()
        .find(|server| server.text_channels.iter().any(|c| c.id == channel_id))
        .context("no text channel with that id; run servers to list them")?;
    let messages = discord
        .history(&server.id, channel_id, yapfast_discord::api::Span::Latest)
        .await?
        .messages;
    // Give the member request a moment, so nicknames show.
    tokio::time::sleep(Duration::from_secs(2)).await;
    for message in &messages {
        let author = discord
            .name(&server.id, &message.author_id)
            .unwrap_or_else(|| message.author.clone());
        let body = match &message.system {
            Some(system) => system.clone(),
            None => discord.render(Some(server), &message.content),
        };
        let edited = if message.edited { " (edited)" } else { "" };
        println!("{author}{edited}: {body}");
        for attachment in &message.attachments {
            println!("  [file] {} {:?}", attachment.name, attachment.size);
        }
        for reaction in &message.reactions {
            println!("  [{}] {}", reaction.key, reaction.count);
        }
    }
    Ok(())
}

/// The first snapshot after connecting, with voice channels this account can join.
async fn online(discord: &Discord) -> Result<Arc<Snapshot>> {
    let mut snapshots = discord.snapshots();
    loop {
        snapshots.changed().await?;
        let snapshot = snapshots.borrow_and_update().clone();
        match &snapshot.status {
            Status::Online => return Ok(snapshot),
            Status::Offline(reason) => bail!("{reason}"),
            Status::Connecting => {}
        }
    }
}

/// Keeps each speaker's audio for the probe to save.
#[derive(Default)]
struct Recorder {
    audio: Mutex<HashMap<u64, Vec<i16>>>,
}

impl Listener for Recorder {
    fn tick(&self, voices: &[(u64, &[i16])]) {
        let mut audio = self.audio.lock().unwrap();
        for (user, samples) in voices {
            audio.entry(*user).or_default().extend_from_slice(samples);
        }
    }

    fn disconnected(&self, reason: &str) {
        println!("Voice disconnected: {reason}");
    }
}

async fn join(channel_id: &str, seconds: u64) -> Result<()> {
    let discord = Discord::start(token::load()?)?;
    let snapshot = online(&discord).await?;
    let mut snapshots = discord.snapshots();
    let (server, channel) = snapshot
        .servers
        .iter()
        .find_map(|server| {
            let channel = server.voice_channels.iter().find(|c| c.id == channel_id)?;
            Some((server.clone(), channel.name.clone()))
        })
        .context("no voice channel with that id; run servers to list them")?;
    println!("Joining {channel} in {}...", server.name);

    let voice_server = discord.join(&server.id, channel_id).await?;
    let recorder = Arc::new(Recorder::default());
    let mut call = Call::connect(&voice_server, &server.id, channel_id, recorder.clone()).await?;
    // The probe talks: show it unmuted.
    discord.set_self(&server.id, channel_id, false, false, false);
    println!("Voice connected.");
    let (microphone, input) = live_microphone();
    call.play(input);
    // Writes the beeps as a microphone would, 20 ms at a time.
    let beeping = tokio::spawn(async move {
        let beeps = beeps(seconds);
        let mut ticks = tokio::time::interval(Duration::from_millis(20));
        for frame in beeps.chunks(FRAME) {
            ticks.tick().await;
            microphone.push(frame);
        }
    });
    println!(
        "Playing a beep every second for {seconds} s. Speak in the channel; Ctrl+C ends early."
    );

    // Show who is in the channel as it changes.
    let mut shown = Vec::new();
    let end = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(end);
    loop {
        let members: Vec<String> = snapshots
            .borrow_and_update()
            .voice
            .get(channel_id)
            .into_iter()
            .flatten()
            .map(|member| member.name.clone())
            .collect();
        if members != shown {
            println!("In the channel: {}", members.join(", "));
            shown = members;
        }
        tokio::select! {
            () = &mut end => break,
            _ = tokio::signal::ctrl_c() => break,
            changed = snapshots.changed() => changed?,
        }
    }

    beeping.abort();
    call.leave();
    discord.leave(&server.id);
    // Let the leave reach Discord before the connection closes.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let names: HashMap<u64, String> = snapshots
        .borrow()
        .voice
        .values()
        .flatten()
        .filter_map(|member| Some((member.user_id.parse().ok()?, member.name.clone())))
        .collect();
    save_recordings(&recorder, &names)
}

/// Write each speaker's audio to a WAV file and summarise it.
fn save_recordings(recorder: &Recorder, names: &HashMap<u64, String>) -> Result<()> {
    let audio = recorder.audio.lock().unwrap();
    if audio.is_empty() {
        println!("Received no audio.");
        return Ok(());
    }
    fs::create_dir_all(RECORDINGS)?;
    for (user, samples) in audio.iter() {
        let label = names.get(user).cloned().unwrap_or_else(|| user.to_string());
        let path = Path::new(RECORDINGS).join(format!("{label}.wav"));
        write_wav(&path, samples)?;
        let seconds = samples.len() as f64 / f64::from(SAMPLE_RATE) / f64::from(CHANNELS);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        let peak_db = 20.0 * (f64::from(peak.max(1)) / 32768.0).log10();
        println!(
            "Received {seconds:.1} s from {label}, peak {peak_db:.1} dBFS: {}",
            path.display()
        );
    }
    Ok(())
}

/// A short 660 Hz beep at the start of every second, as mono f32, the
/// microphone's format.
fn beeps(seconds: u64) -> Vec<f32> {
    let rate = SAMPLE_RATE as usize;
    let frames = rate * seconds as usize;
    let beep = rate * 3 / 10;
    let fade = rate / 200;
    (0..frames)
        .map(|frame| {
            let t = frame % rate;
            if t < beep {
                let envelope = (t.min(beep - t) as f32 / fade as f32).min(1.0);
                let phase = 2.0 * std::f32::consts::PI * 660.0 * frame as f32 / rate as f32;
                0.25 * envelope * phase.sin()
            } else {
                0.0
            }
        })
        .collect()
}

fn write_wav(path: &Path, samples: &[i16]) -> Result<()> {
    let data_len = u32::try_from(samples.len() * 2)?;
    let block_align = CHANNELS * 2;
    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&SAMPLE_RATE.to_le_bytes())?;
    file.write_all(&(SAMPLE_RATE * u32::from(block_align)).to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// Read one line from the terminal without echoing it.
fn read_hidden(prompt: &str) -> Result<String> {
    let tty = || File::options().read(true).write(true).open("/dev/tty");
    let stty = |arg: &str| -> Result<()> {
        let status = Command::new("stty")
            .arg(arg)
            .stdin(tty().context("needs a terminal")?)
            .stdout(Stdio::null())
            .status()
            .context("could not run stty")?;
        if !status.success() {
            bail!("could not hide terminal input");
        }
        Ok(())
    };
    let mut terminal = tty().context("needs a terminal")?;
    write!(terminal, "{prompt}")?;
    terminal.flush()?;
    stty("-echo")?;
    let mut line = String::new();
    let read = std::io::BufReader::new(tty()?).read_line(&mut line);
    stty("echo")?;
    writeln!(terminal)?;
    read.context("could not read the token")?;
    Ok(line)
}
