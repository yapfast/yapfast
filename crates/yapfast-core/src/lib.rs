//! Yapfast's application state and native network services, independent of the UI.
//! Matrix and Discord are the `matrix` and `discord` Cargo features.
// Matrix sync's encrypted-event futures exceed rustc's default trait depth.
#![recursion_limit = "256"]
// A build without networks only runs the preview and device checks.
#![cfg_attr(not(any(feature = "matrix", feature = "discord")), allow(dead_code))]

use anyhow::Context;
/// The Discord protocol, for clients that hand the application Discord's
/// state themselves, as the UI's tests do.
#[cfg(feature = "discord")]
pub use yapfast_discord;

pub mod application;
mod audio_io;
pub mod backend;
#[cfg(feature = "matrix")]
pub mod calls;
pub mod camera;
pub mod chat;
pub mod cli;
#[cfg(feature = "discord")]
pub mod clip;
pub mod clock;
#[cfg(feature = "discord")]
mod declick;
#[cfg(feature = "discord")]
pub mod decode;
#[cfg(feature = "discord")]
mod dereverb;
pub mod discord;
#[cfg(feature = "discord")]
pub mod discord_audio;
#[cfg(feature = "discord")]
pub mod discord_link;
#[cfg(feature = "discord")]
pub mod discord_text;
#[cfg(feature = "discord")]
mod discord_video;
#[cfg(feature = "discord")]
mod discord_voice;
#[cfg(feature = "discord")]
pub mod encode;
#[cfg(feature = "matrix")]
pub mod experiment;
pub mod frames;
pub mod leveling;
pub mod links;
pub mod logging;
#[cfg(feature = "discord")]
mod lottie;
#[cfg(feature = "matrix")]
mod matrix_chat;
#[cfg(feature = "matrix")]
mod matrix_media;
#[cfg(feature = "matrix")]
mod matrix_video;
#[cfg(feature = "matrix")]
mod matrix_voice;
pub mod mention;
pub mod meter;
pub mod network;
pub mod noise;
pub mod picture;
#[cfg(feature = "discord")]
pub mod playback;
pub mod push_to_talk;
#[cfg(feature = "discord")]
pub mod ring;
pub mod runtime;
#[cfg(feature = "matrix")]
pub mod session;
pub mod sounds;
pub mod spoiler;
#[cfg(feature = "discord")]
pub mod stream_audio;
#[cfg(feature = "discord")]
pub mod stream_worker;
#[cfg(feature = "discord")]
pub mod video;
#[cfg(feature = "discord")]
pub mod video_player;
pub mod voice;
#[cfg(feature = "matrix")]
pub mod voice_demo;
pub mod volume;

/// Install the TLS provider before starting Matrix or calling services.
pub fn init_tls() -> anyhow::Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("could not initialize the TLS provider"))
}

/// Where Yapfast keeps its sessions and local settings unless given --data-dir.
pub fn default_directory() -> anyhow::Result<std::path::PathBuf> {
    directories::ProjectDirs::from("io.github", "yapfast", "Yapfast")
        .map(|dirs| dirs.data_local_dir().to_owned())
        .context("could not find an application data directory; pass --data-dir")
}

/// Keeps the cookies Discord sets, and the account as last seen, in `root`
/// across restarts.
#[cfg(feature = "discord")]
pub fn keep_discord_files(root: &std::path::Path) {
    yapfast_discord::cookies::keep_in(root.join("discord-cookies.json"));
    yapfast_discord::cache::keep_in(root.join("discord-account.json"));
}

/// Check linked native services without a session, network request or audio device.
pub async fn check_build() {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    #[cfg(feature = "matrix")]
    {
        calls::check_native_build();
        let _ = matrix_sdk::Client::builder();
    }
    #[cfg(feature = "discord")]
    {
        let _ = yapfast_discord::voice::live_microphone();
        check_opus();
    }
}

/// Encodes a tone with Opus and decodes it again. Discord calls' Opus is the
/// libopus inside libwebrtc's archive, called directly, so this checks it
/// links and works on each platform.
#[cfg(feature = "discord")]
fn check_opus() {
    const RATE: u32 = 48_000;
    const FRAME: usize = RATE as usize / 50;
    let tone: Vec<f32> = (0..FRAME)
        .flat_map(|at| {
            let sample = 0.5 * (at as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin();
            [sample, sample]
        })
        .collect();
    let mut encoder =
        yapfast_discord::opus::Encoder::new(2, yapfast_discord::opus::Application::Audio, 128_000)
            .expect("an Opus encoder");
    let mut decoder = yapfast_discord::opus::Decoder::new(2).expect("an Opus decoder");
    let mut packet = [0u8; 4000];
    let mut decoded = vec![0i16; FRAME * 2];
    // The first frames are mostly the encoder's lookahead.
    for _ in 0..5 {
        let size = encoder
            .encode_float(&tone, &mut packet)
            .expect("Opus encodes");
        let frames = decoder
            .decode(&packet[..size], &mut decoded, false)
            .expect("Opus decodes");
        assert_eq!(frames, FRAME, "Opus decoded the wrong length");
    }
    let peak = decoded.iter().fold(0f32, |peak, &sample| {
        peak.max(f32::from(sample).abs() / 32_768.0)
    });
    assert!(peak > 0.3, "Opus lost the tone: its peak is {peak}");
}

/// Cargo runs library tests from the crate directory; retain workspace-local fixtures.
#[cfg(all(test, feature = "matrix"))]
fn test_data_directory() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".local")
}
