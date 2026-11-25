//! Opt-in tests against the real Windows audio engine, with quiet test tones.
use super::*;
use crate::voice::StreamAudio;
use std::{
    io::{BufRead, BufReader},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
};

struct Tone(Child);

impl Drop for Tone {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_tone(directory: &std::path::Path, hz: u32) -> (Tone, u64) {
    let wave = directory.join(format!("{hz}.wav"));
    let bytes = RATE * 4;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for at in 0..RATE {
        let sample =
            (327.0 * (at as f32 * hz as f32 * std::f32::consts::TAU / RATE as f32).sin()) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(&wave, wav).unwrap();
    let script = directory.join("play.ps1");
    std::fs::write(
        &script,
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$handle = $form.Handle.ToInt64()
$player = New-Object System.Media.SoundPlayer $args[0]
$player.Load()
$player.PlayLooping()
[Console]::WriteLine($handle)
[Console]::Out.Flush()
[Console]::ReadLine() | Out-Null
$player.Stop()
$form.Dispose()
"#,
    )
    .unwrap();
    let child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg(wave)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // No visible console or form; only its window handle is queried.
        .creation_flags(0x0800_0000)
        .spawn()
        .unwrap();
    let mut child = Tone(child);
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let window = line
        .trim()
        .parse()
        .expect("the tone process supplies its window handle");
    (child, window)
}

struct Listening {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

/// Mean power at a tone's frequency, independently in each 20 ms frame so
/// capture jitter doesn't cancel the measurements across frame boundaries.
fn power(frames: &[f32], hz: u32) -> f64 {
    let mut total = 0.0;
    for frame in frames.as_chunks::<FRAME>().0 {
        let (mut real, mut imaginary) = (0.0, 0.0);
        for (at, sample) in frame.iter().step_by(2).enumerate() {
            let phase = at as f64 * hz as f64 * std::f64::consts::TAU / RATE as f64;
            real += f64::from(*sample) * phase.cos();
            imaginary += f64::from(*sample) * phase.sin();
        }
        total += (real * real + imaginary * imaginary) / (FRAME / 2).pow(2) as f64;
    }
    total / (frames.len() / FRAME).max(1) as f64
}

fn record(audio: StreamAudio) -> (Vec<f32>, Vec<f32>) {
    let inputs = Inputs::default();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = listen(inputs.clone(), stop.clone(), audio).expect("Windows audio capture opens");
    let _listening = Listening {
        stop,
        thread: Some(thread),
    };
    std::thread::sleep(Duration::from_millis(200));
    let mut encoder = opus::Encoder::new(2, opus::Application::Audio, BITRATE).unwrap();
    let mut decoder = opus::Decoder::new(2).unwrap();
    let mut raw = Vec::new();
    let mut decoded = Vec::new();
    let mut frame = vec![0.0; FRAME];
    let mut packet = vec![0; 4000];
    let mut pcm = vec![0i16; FRAME];
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(20));
        if mix(&mut inputs.lock().unwrap(), &mut frame) {
            raw.extend_from_slice(&frame);
            let size = encoder.encode_float(&frame, &mut packet).unwrap();
            let samples = decoder.decode(&packet[..size], &mut pcm, false).unwrap();
            decoded.extend(
                pcm[..samples * 2]
                    .iter()
                    .map(|sample| f32::from(*sample) / 32768.0),
            );
        }
    }
    assert!(!raw.is_empty(), "captured non-silent audio");
    (raw, decoded)
}

#[test]
#[ignore = "plays quiet tones; needs a Windows desktop with working speakers"]
fn captures_and_encodes_selected_application_audio_on_windows() {
    if let Ok(windows) = std::env::var("YAPFAST_AUDIO_PROBE_WINDOWS") {
        let (first, second) = windows.split_once(':').unwrap();
        verify_selection(first.parse().unwrap(), second.parse().unwrap());
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let (_first, first) = start_tone(directory.path(), 733);
    let (_second, second) = start_tone(directory.path(), 1511);
    // All-app capture excludes Yapfast's entire process tree. Put the
    // capturer in a differently named sibling of the tone processes so its
    // own-process exclusion doesn't intentionally exclude both test tones.
    let capturer = directory.path().join("audio-window-capture.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &capturer).unwrap();
    let output = Command::new(capturer)
        .args([
            "captures_and_encodes_selected_application_audio_on_windows",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("YAPFAST_AUDIO_PROBE_WINDOWS", format!("{first}:{second}"))
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    eprintln!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "the independent audio capturer passed"
    );
}

fn verify_selection(first: u64, second: u64) {
    for audio in [
        StreamAudio::All,
        StreamAudio::Window(first),
        StreamAudio::Window(second),
        StreamAudio::All,
    ] {
        let (raw, decoded) = record(audio);
        for (stage, samples) in [("capture", raw), ("Opus round trip", decoded)] {
            let first_power = power(&samples, 733);
            let second_power = power(&samples, 1511);
            eprintln!("{audio:?}, {stage}: 733 Hz={first_power:.9}, 1511 Hz={second_power:.9}");
            match audio {
                StreamAudio::All => assert!(
                    first_power > 1e-9 && second_power > 1e-9,
                    "both applications are audible"
                ),
                StreamAudio::Window(window) if window == first => assert!(
                    first_power > 1e-9 && second_power < first_power * 0.1,
                    "only the selected first application's audio"
                ),
                StreamAudio::Window(_) => assert!(
                    second_power > 1e-9 && first_power < second_power * 0.1,
                    "only the selected second application's audio"
                ),
                StreamAudio::Off => unreachable!(),
            }
        }
        // Stopping capture releases its thread; Off opens no replacement.
        assert!(
            listen(
                Inputs::default(),
                Arc::new(AtomicBool::new(false)),
                StreamAudio::Off
            )
            .is_err()
        );
    }
}
