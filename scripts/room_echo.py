"""Puts the noise comparison's speech through synthetic rooms, and scores the
Discord microphone with and without taking the room's echo out.
scripts/compare-room-echo runs it; see the Microphone section of
docs/discord/voice.md.

  room_echo.py make SPEECH OUT   SPEECH is the noise comparison's speech.f32
  room_echo.py score OUT NOISE   NOISE holds the noise comparison's mixes
"""
import sys
from pathlib import Path

import numpy as np
from pesq import pesq
from scipy.signal import correlate, fftconvolve, resample_poly

RATE = 48000
BLOCK = 480
# Reverberation time in seconds, and how much quieter the reverb is than the
# direct sound in total, in dB.
ROOMS = {"dry": None, "rt40": (0.4, 6.0), "rt60": (0.6, 3.0), "rt80": (0.8, 0.0)}
NOISE = ["typing", "fan", "hiss", "typing_fan"]


def blocks_db(x):
    n = len(x) // BLOCK
    return 10 * np.log10(np.mean(x[:n * BLOCK].reshape(n, BLOCK) ** 2, axis=1) + 1e-12)


def power_db(levels):
    return 10 * np.log10(np.mean(10 ** (levels / 10)) + 1e-12)


def room(rt60, drr_db):
    """Direct sound, then a diffuse tail decaying 60 dB in rt60."""
    length = int(rt60 * 1.2 * RATE)
    t = np.arange(length) / RATE
    tail = np.random.default_rng(9).standard_normal(length) * 10 ** (-3 * t / rt60)
    tail[: int(0.003 * RATE)] = 0
    tail *= np.sqrt(10 ** (-drr_db / 10) / np.sum(tail ** 2))
    tail[0] = 1.0
    return tail


def make(speech, out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    dry = np.fromfile(speech, np.float32).astype(np.float64)
    for name, params in ROOMS.items():
        x = dry if params is None else fftconvolve(dry, room(*params))[: len(dry)]
        levels = blocks_db(x)
        x *= 10 ** ((-26 - power_db(levels[levels > -60])) / 20)
        x.astype(np.float32).tofile(out / f"{name}.f32")
        (np.clip(x, -1, 1) * 32767).astype("<i2").tofile(out / f"{name}.s16")
    speaking = blocks_db(dry) > -50
    # Phrase ends: speech, then at least 500 ms of pause.
    ends = [i for i in range(1, len(speaking) - 50)
            if speaking[i - 1] and not speaking[i:i + 50].any()]
    np.save(out / "ends.npy", np.array(ends))
    np.save(out / "speaking.npy", speaking)


def align(x, reference):
    window = 20 * RATE
    c = correlate(x[:window], reference[:window], mode="full", method="fft")
    lag = int(np.argmax(c) - (window - 1))
    x = x[lag:] if lag > 0 else np.concatenate([np.zeros(-lag), x])
    return np.concatenate([x, np.zeros(max(0, len(reference) - len(x)))])[: len(reference)]


def tails(x, ends, speaking):
    """The level 50-150 ms and 150-400 ms after phrases, against the speech's."""
    levels = blocks_db(x)
    speech = power_db(levels[: len(speaking)][speaking])
    early = np.concatenate([levels[e + 5:e + 15] for e in ends])
    late = np.concatenate([levels[e + 15:e + 40] for e in ends])
    return power_db(early) - speech, power_db(late) - speech


def wideband_pesq(reference, degraded):
    return pesq(16000, resample_poly(reference, 1, 3), resample_poly(degraded, 1, 3), "wb")


def score(out, noise):
    out, noise = Path(out), Path(noise)
    read16 = lambda f: np.fromfile(f, "<i2").astype(np.float64) / 32768
    dry = np.fromfile(out / "dry.f32", np.float32).astype(np.float64)
    ends, speaking = np.load(out / "ends.npy"), np.load(out / "speaking.npy")
    print("Rooms: PESQ-WB against the dry speech, and the reverb tail 50-150 / 150-400 ms")
    print("after phrases, in dB against the speech.")
    print(f"{'':10}{'unprocessed':>24}{'room echo left':>24}{'room echo out':>24}")
    for name in ROOMS:
        row = f"{name:10}"
        for x in [np.fromfile(out / f"{name}.f32", np.float32).astype(np.float64),
                  read16(out / f"chain/{name}_off.s16"), read16(out / f"chain/{name}_on.s16")]:
            x = align(x, dry)
            early, late = tails(x, ends, speaking)
            row += f"{wideband_pesq(dry, x):8.2f} {early:6.1f} / {late:5.1f}"
        print(row)
    speech = np.fromfile(noise / "speech.f32", np.float32).astype(np.float64)
    print("\nNoise mixes: PESQ-WB against the clean speech.")
    print(f"{'':12}{'room echo left':>16}{'room echo out':>16}")
    for name in NOISE:
        scores = [wideband_pesq(speech, align(read16(out / f"chain/{name}_{d}.s16"), speech))
                  for d in ["off", "on"]]
        print(f"{name:12}{scores[0]:16.2f}{scores[1]:16.2f}")


if __name__ == "__main__":
    {"make": make, "score": score}[sys.argv[1]](*sys.argv[2:])
