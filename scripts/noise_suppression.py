"""Mixes speech with keyboard and fan noise, and scores the Discord
microphone's noise suppressors on the mixes. scripts/compare-noise-suppression
runs it; see the Microphone section of docs/discord/voice.md.

  noise_suppression.py mix RAW MIXES     RAW holds the decoded sources
  noise_suppression.py score MIXES OUT   OUT holds the tests' outputs
"""
import re
import sys
from pathlib import Path

import numpy as np
from pesq import pesq
from scipy.signal import butter, correlate, resample_poly, sosfilt

RATE = 48000
BLOCK = 480
SUPPRESSORS = ["rnnoise", "deepfilternet"]
MIXES = ["clean", "typing", "fan", "hiss", "typing_fan", "typing_hiss",
         "quiet_typing_fan", "quiet_typing_hiss"]


def blocks_db(x):
    n = len(x) // BLOCK
    return 10 * np.log10(np.mean(x[:n * BLOCK].reshape(n, BLOCK) ** 2, axis=1) + 1e-12)


def power_db(levels):
    return 10 * np.log10(np.mean(10 ** (levels / 10)))


def read_f32(path):
    return np.fromfile(path, np.float32).astype(np.float64)


def mix(raw, out):
    raw, out = Path(raw), Path(out)
    rng = np.random.default_rng(7)
    total = 90 * RATE
    # Speech: 3-7 s from two readings in turn, with 1.5-4 s pauses between.
    sources = [read_f32(raw / f"{name}.f32")[30 * RATE:] for name in ["speech-1", "speech-2"]]
    speech, pause = np.zeros(total), np.zeros(total, bool)
    at, read = RATE, [0, 0]
    pause[:at] = True
    turn = 0
    while True:
        length, gap = int(rng.uniform(3, 7) * RATE), int(rng.uniform(1.5, 4) * RATE)
        if at + length + gap > total:
            pause[at:] = True
            break
        source = turn % 2
        speech[at:at + length] = sources[source][read[source]:read[source] + length]
        read[source] += length + RATE // 2
        at += length
        pause[at:at + gap] = True
        at += gap
        turn += 1
    levels = blocks_db(speech)
    speech *= 10 ** ((-26 - power_db(levels[levels > -60])) / 20)

    highpass = butter(2, 100, "highpass", fs=RATE, output="sos")
    typing = np.concatenate([read_f32(raw / f"typing-{index}.f32") for index in range(1, 5)])
    typing = np.resize(typing[5 * RATE:] - typing.mean(), total)
    typing *= 10 ** ((-32 - np.percentile(blocks_db(typing), 95)) / 20)
    # WebRTC's high-pass filter takes out the rumble before the suppressor.
    fan = read_f32(raw / "fan.f32")[2 * RATE:]
    fan = sosfilt(highpass, np.resize(fan - fan.mean(), total))
    fan *= 10 ** ((-42 - 10 * np.log10(np.mean(fan ** 2))) / 20)
    spectrum = np.fft.rfft(rng.standard_normal(total))
    spectrum[1:] /= np.sqrt(np.fft.rfftfreq(total, 1 / RATE)[1:])
    spectrum[0] = 0
    hiss = sosfilt(highpass, np.fft.irfft(spectrum, total))
    hiss *= 10 ** ((-45 - 10 * np.log10(np.mean(hiss ** 2))) / 20)

    quiet = 10 ** (-14 / 20)
    mixes = {
        "clean": speech,
        "typing": speech + typing,
        "fan": speech + fan,
        "hiss": speech + hiss,
        "typing_fan": speech + typing + fan,
        "typing_hiss": speech + typing + hiss,
        "quiet_typing_fan": (speech + typing + fan) * quiet,
        "quiet_typing_hiss": (speech + typing + hiss) * quiet,
    }
    out.mkdir(parents=True, exist_ok=True)
    for name, x in mixes.items():
        x.astype(np.float32).tofile(out / f"{name}.f32")
        np.clip(x * 32768, -32768, 32767).astype("<i2").tofile(out / f"{name}.s16")
    speech.astype(np.float32).tofile(out / "speech.f32")
    # Each 10 ms block: 1 speech, 0 a pause at least 200 ms from speech, -1 neither.
    n = total // BLOCK
    paused = pause[:n * BLOCK].reshape(n, BLOCK).all(axis=1)
    near_speech = np.convolve(~paused, np.ones(41), "same") > 0
    truth = np.where(blocks_db(speech) > -55, 1, np.where(paused & ~near_speech, 0, -1))
    truth.astype(np.int8).tofile(out / "truth.i8")


def lag(output, reference):
    window = 30 * RATE
    c = correlate(output[:window], reference[:window], mode="full", method="fft")
    return int(np.argmax(c) - (window - 1))


def si_sdr(estimate, reference):
    target = np.dot(estimate, reference) / np.dot(reference, reference) * reference
    error = estimate - target
    return 10 * np.log10(np.dot(target, target) / max(np.dot(error, error), 1e-20))


def wideband_pesq(reference, degraded):
    return pesq(16000, resample_poly(reference, 1, 3), resample_poly(degraded, 1, 3), "wb")


def sent_blocks(log, blocks):
    """The blocks the gate sent, from processes_a_recording's output."""
    sent, start = np.zeros(blocks, bool), None
    for line in open(log):
        if opened := re.match(r"opened at ([\d.]+) s, sending (\d+) blocks", line):
            start = round(float(opened[1]) * 100)
            sent[start - int(opened[2]) + 1:start + 1] = True
        elif closed := re.match(r"closed at ([\d.]+) s", line):
            sent[start:round(float(closed[1]) * 100)] = True
            start = None
    if start is not None:
        sent[start:] = True
    return sent


def score(mixes, out):
    mixes, out = Path(mixes), Path(out)
    speech, truth = read_f32(mixes / "speech.f32"), np.fromfile(mixes / "truth.i8", np.int8)
    lags = {"noisy": 0}
    for suppressor in SUPPRESSORS:
        lags[suppressor] = lag(read_f32(out / f"alone/clean_{suppressor}.f32"), speech)
        print(f"{suppressor}: {lags[suppressor] / RATE * 1000:.0f} ms behind its input")

    print("\nThe suppressors alone. Pauses: the output level between utterances, in dBFS.")
    print(f"{'mix':18} {'':14} {'PESQ-WB':>7} {'SI-SDR':>7} {'pause mean':>10} "
          f"{'pause p99':>9} {'pause max':>9} {'speech':>7}")
    reference = blocks_db(speech)[truth == 1]
    for name in [name for name in MIXES if not name.startswith("quiet")]:
        for path in ["noisy"] + SUPPRESSORS:
            if name == "clean" and path == "noisy":
                continue
            source = mixes / f"{name}.f32" if path == "noisy" else out / f"alone/{name}_{path}.f32"
            x = read_f32(source)[lags[path]:]
            x = np.concatenate([x, np.zeros(len(speech) - len(x))])
            levels = blocks_db(x)
            pauses = levels[truth == 0]
            change = power_db(levels[truth == 1]) - power_db(reference)
            print(f"{name:18} {path:14} {wideband_pesq(speech, x):7.2f} {si_sdr(x, speech):7.1f} "
                  f"{power_db(pauses):10.1f} {np.percentile(pauses, 99):9.1f} "
                  f"{pauses.max():9.1f} {change:+6.1f} dB")

    print("\nThe whole microphone, with echo cancellation, gain control and the gate.")
    print(f"{'mix':18} {'':14} {'speech sent':>11} {'pauses sent':>11} {'opened in pauses':>16}")
    for name in MIXES:
        for suppressor in SUPPRESSORS + ["off"]:
            log = out / f"chain/{name}_{suppressor}.log"
            sent = sent_blocks(log, len(truth))
            opened = sum(truth[round(float(m[1]) * 100)] == 0 for m in
                         re.finditer(r"opened at ([\d.]+) s", log.read_text()))
            print(f"{name:18} {suppressor:14} {np.mean(sent[truth == 1]) * 100:10.1f}% "
                  f"{np.mean(sent[truth == 0]) * 100:10.1f}% {opened:16d}")


if __name__ == "__main__":
    {"mix": mix, "score": score}[sys.argv[1]](*sys.argv[2:])
