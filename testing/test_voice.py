#!/usr/bin/env python3
"""Exercise native audio with isolated virtual devices and a generated tone."""
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import wave

for tool in ("pactl", "pw-play", "parec", "cargo"):
    if not shutil.which(tool):
        raise SystemExit(f"{tool} is required. Run inside the development shell on a Linux desktop with PipeWire.")

modules = []
tone = None
try:
    for args in (
        ["module-null-sink", "sink_name=yapfast_test_input", "sink_properties=device.description=Yapfast_test_input", "rate=48000"],
        ["module-remap-source", "master=yapfast_test_input.monitor", "source_name=yapfast_test_microphone", "source_properties=device.description=Yapfast_test_microphone"],
        ["module-null-sink", "sink_name=yapfast_test_output", "sink_properties=device.description=Yapfast_test_speakers", "rate=48000"],
    ):
        modules.append(subprocess.check_output(["pactl", "load-module", *args], text=True).strip())
    with tempfile.TemporaryDirectory(prefix="yapfast-voice-") as directory:
        path = Path(directory) / "tone.wav"
        # Only generated samples enter these test devices.
        second = b"".join(struct.pack("<h", round(4000 * math.sin(2 * math.pi * 440 * i / 48000))) for i in range(48000))
        with wave.open(str(path), "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(48000)
            for _ in range(180):
                output.writeframesraw(second)
        tone = subprocess.Popen(["pw-play", "--volume=1.0", "--target=yapfast_test_input", str(path)])
        # Limit even a library's default-device fallback to these virtual devices.
        env = dict(os.environ, PULSE_SOURCE="yapfast_test_microphone", PULSE_SINK="yapfast_test_output")
        if "--fast" in sys.argv:
            env["YAPFAST_TEST_FAST"] = "1"
        result = subprocess.run(["cargo", "test", "--locked", "--profile", "fast", "-p", "yapfast-core", "--features", "matrix", "matrix_voice::tests::persistent_encrypted_voice_and_native_audio", "--", "--ignored", "--nocapture"], env=env)
        raise SystemExit(result.returncode)
finally:
    if tone:
        tone.terminate()
        tone.wait(timeout=5)
    for module in reversed(modules):
        subprocess.run(["pactl", "unload-module", module], check=False)
