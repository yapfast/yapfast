"""The plugin measurement harness: frames, helper exchange, metrics and labels.

Every test runs against stdlib only. The helper is simulated by a small Python
script speaking the documented framed JSON protocol, so the harness is
exercised before the real `yapfast-plugins` host exists.
"""
import importlib.util
import io
import json
import os
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "testing/measure_plugins.py"

spec = importlib.util.spec_from_file_location("measure_plugins", MODULE_PATH)
if spec is None or spec.loader is None:
    raise ImportError(f"cannot load {MODULE_PATH}")
mp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mp)


FAKE_HELPER = r'''
import json
import struct
import sys

HEADER = struct.Struct("<IBQ")


def read_exact(count, clean_eof=False):
    data = b""
    while len(data) < count:
        chunk = sys.stdin.buffer.read(count - len(data))
        if not chunk:
            if clean_eof and not data:
                return None
            raise EOFError("stream ended mid-frame")
        data += chunk
    return data


def read_frame():
    header = read_exact(HEADER.size, clean_eof=True)
    if header is None:
        return None
    length, kind, frame_id = HEADER.unpack(header)
    payload = read_exact(length) if length else b""
    return kind, frame_id, json.loads(payload) if payload else None


def write_frame(kind, frame_id, body):
    payload = json.dumps(body, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(HEADER.pack(len(payload), kind, frame_id) + payload)
    sys.stdout.buffer.flush()


write_frame(1, 0, {"type": "ready"})
while True:
    frame = read_frame()
    if frame is None:
        break
    kind, frame_id, request = frame
    request_type = request.get("type") if isinstance(request, dict) else None
    if request_type == "load":
        write_frame(3, frame_id, {"type": "loaded"})
    elif request_type == "call":
        write_frame(5, frame_id, {"type": "reply", "payload": request.get("payload")})
    elif request_type == "unload":
        write_frame(7, frame_id, {"type": "unloaded"})
    elif request_type == "shutdown":
        break
    else:
        write_frame(9, frame_id, {"type": "error", "message": "unknown request"})
'''


# The protocol allows the helper to exit when the last instance is unloaded.
EXIT_ON_UNLOAD_HELPER = FAKE_HELPER.replace(
    '        write_frame(7, frame_id, {"type": "unloaded"})',
    '        write_frame(7, frame_id, {"type": "unloaded"})\n        break',
)


SILENT_HELPER = r'''
import struct
import sys
import time

sys.stdout.buffer.write(struct.pack("<IBQ", 15, 1, 0) + b'{"type":"ready"}')
sys.stdout.buffer.flush()
time.sleep(10)
'''


WRONG_ID_HELPER = r'''
import json
import struct
import sys

HEADER = struct.Struct("<IBQ")


def read_exact(count):
    data = b""
    while len(data) < count:
        chunk = sys.stdin.buffer.read(count - len(data))
        if not chunk:
            return None
        data += chunk
    return data


def write_frame(kind, frame_id, body):
    payload = json.dumps(body, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(HEADER.pack(len(payload), kind, frame_id) + payload)
    sys.stdout.buffer.flush()


write_frame(1, 0, {"type": "ready"})
header = read_exact(HEADER.size)
length, kind, frame_id = HEADER.unpack(header)
read_exact(length)
write_frame(5, frame_id + 1, {"type": "reply", "payload": None})
'''


WRONG_TYPE_HELPER = r'''
import json
import struct
import sys

HEADER = struct.Struct("<IBQ")


def read_exact(count):
    data = b""
    while len(data) < count:
        chunk = sys.stdin.buffer.read(count - len(data))
        if not chunk:
            return None
        data += chunk
    return data


def write_frame(kind, frame_id, body):
    payload = json.dumps(body, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(HEADER.pack(len(payload), kind, frame_id) + payload)
    sys.stdout.buffer.flush()


write_frame(1, 0, {"type": "ready"})
header = read_exact(HEADER.size)
length, kind, frame_id = HEADER.unpack(header)
read_exact(length)
write_frame(5, frame_id, {"type": "ready"})
'''


class Trickle:
    """A reader that hands back at most `step` bytes per read, as a pipe may."""

    def __init__(self, data, step=1):
        self.data = data
        self.step = step

    def read(self, count=-1):
        if not self.data:
            return b""
        take = min(self.step, len(self.data), count if count >= 0 else len(self.data))
        chunk, self.data = self.data[:take], self.data[take:]
        return chunk


class FrameCodecTest(unittest.TestCase):
    def test_wire_is_length_kind_id_payload(self):
        payload = b'{"method":"shout"}'
        frame_id = 0x0102030405060708
        wire = mp.encode_frame(mp.Kind.CALL, frame_id, payload)

        self.assertEqual(wire[:4], struct.pack("<I", len(payload)))
        self.assertEqual(wire[4], 4)
        self.assertEqual(wire[5:13], struct.pack("<Q", frame_id))
        self.assertEqual(wire[13:], payload)
        self.assertEqual(len(wire), mp.HEADER_LEN + len(payload))
        self.assertEqual(mp.HEADER_LEN, 13)

    def test_empty_payload_is_a_header_only_frame(self):
        wire = mp.encode_frame(mp.Kind.SHUTDOWN, 0, b"")
        self.assertEqual(len(wire), mp.HEADER_LEN)
        frame = mp.read_frame(io.BytesIO(wire))
        self.assertEqual(frame, mp.Frame(mp.Kind.SHUTDOWN, 0, b""))
        self.assertEqual(mp.read_frame(io.BytesIO(wire[:0])), None)

    def test_round_trip_non_ascii_json(self):
        payload = '{"text":"Ærø — ☃","emoji":"🎧"}'.encode("utf-8")
        wire = mp.encode_frame(mp.Kind.REPLY, u64 := 2**64 - 1, payload)
        frame = mp.read_frame(io.BytesIO(wire))
        self.assertEqual(frame.kind, mp.Kind.REPLY)
        self.assertEqual(frame.id, u64)
        self.assertEqual(frame.payload, payload)

    def test_fragmented_stream_is_reassembled(self):
        first = mp.encode_frame(mp.Kind.LOAD, 1, b'{"plugin":"echo"}')
        second = mp.encode_frame(mp.Kind.SHUTDOWN, 2, b"{}")
        reader = Trickle(first + second, step=1)

        self.assertEqual(mp.read_frame(reader).id, 1)
        self.assertEqual(mp.read_frame(reader).id, 2)
        self.assertIsNone(mp.read_frame(reader))

    def test_two_frames_in_one_write_are_read_in_order(self):
        data = mp.encode_frame(mp.Kind.READY, 0, b'{"type":"ready"}')
        data += mp.encode_frame(mp.Kind.REPLY, 9, b"{}")
        stream = io.BytesIO(data)
        self.assertEqual(mp.read_frame(stream).kind, mp.Kind.READY)
        self.assertEqual(mp.read_frame(stream).kind, mp.Kind.REPLY)
        self.assertIsNone(mp.read_frame(stream))

    def test_clean_eof_at_a_frame_boundary_is_none(self):
        self.assertIsNone(mp.read_frame(io.BytesIO(b"")))
        one = mp.encode_frame(mp.Kind.READY, 0, b"{}")
        stream = io.BytesIO(one)
        self.assertIsNotNone(mp.read_frame(stream))
        self.assertIsNone(mp.read_frame(stream))

    def test_eof_inside_the_header_is_unexpected(self):
        wire = mp.encode_frame(mp.Kind.CALL, 7, b"{}")
        for cut in range(1, mp.HEADER_LEN):
            with self.assertRaises(mp.ReadError) as raised:
                mp.read_frame(io.BytesIO(wire[:cut]))
            self.assertEqual(raised.exception.expected, mp.HEADER_LEN)
            self.assertEqual(raised.exception.got, cut)

    def test_eof_inside_the_payload_is_unexpected(self):
        wire = mp.encode_frame(mp.Kind.CALL, 7, b"0123456789")
        with self.assertRaises(mp.ReadError) as raised:
            mp.read_frame(io.BytesIO(wire[:-1]))
        self.assertEqual(raised.exception.expected, 10)
        self.assertEqual(raised.exception.got, 9)

    def test_oversize_declared_length_is_refused_before_the_payload(self):
        for declared in (mp.MAX_PAYLOAD_LEN + 1, 0xFFFFFFFF):
            header = struct.pack("<IBQ", declared, mp.Kind.CALL, 0)
            with self.assertRaises(mp.ReadError) as raised:
                mp.read_frame(io.BytesIO(header))
            self.assertEqual(raised.exception.declared, declared)

    def test_an_unknown_kind_is_refused(self):
        for byte in (0, 10, 42, 255):
            header = struct.pack("<IBQ", 0, byte, 0)
            with self.assertRaises(mp.ReadError) as raised:
                mp.read_frame(io.BytesIO(header))
            self.assertEqual(raised.exception.kind, byte)

    def test_encoding_refuses_a_payload_past_the_frame_cap(self):
        self.assertEqual(
            len(mp.encode_frame(mp.Kind.CALL, 0, b"x" * mp.MAX_PAYLOAD_LEN)),
            mp.MAX_FRAME_LEN,
        )
        with self.assertRaises(mp.FrameError):
            mp.encode_frame(mp.Kind.CALL, 0, b"x" * (mp.MAX_PAYLOAD_LEN + 1))


class PercentileTest(unittest.TestCase):
    def test_nearest_rank_uses_the_element_at_ceil_p_over_100_n(self):
        values = list(range(1, 11))
        self.assertEqual(mp.nearest_rank(values, 50), 5)
        self.assertEqual(mp.nearest_rank(values, 95), 10)
        self.assertEqual(mp.nearest_rank(values, 99), 10)
        self.assertEqual(mp.nearest_rank(values, 100), 10)
        self.assertEqual(mp.nearest_rank(values, 1), 1)
        self.assertEqual(mp.nearest_rank([10, 20, 30, 40], 50), 20)
        self.assertEqual(mp.nearest_rank([7], 99), 7)

    def test_invalid_percentiles_and_empty_input_are_refused(self):
        for percent in (0, -1, 101):
            with self.assertRaises(ValueError):
                mp.nearest_rank([1, 2], percent)
        with self.assertRaises(ValueError):
            mp.nearest_rank([], 50)

    def test_summary_reports_p50_p95_p99(self):
        summary = mp.percentiles([50, 10, 40, 20, 30])
        self.assertEqual(summary, {"p50": 30, "p95": 50, "p99": 50})


class ProcStatusTest(unittest.TestCase):
    STATUS = """\
Name:\thelper
VmPeak:\t  102400 kB
VmHWM:\t   20480 kB
VmRSS:\t    6400 kB
RssAnon:\t    6144 kB
RssFile:\t     256 kB
VmSwap:\t     128 kB
Threads:\t       5
"""

    def test_parses_rss_anon_vm_swap_and_lifetime_peak(self):
        memory = mp.parse_proc_status(self.STATUS)
        self.assertEqual(memory["rss_anon_kib"], 6144)
        self.assertEqual(memory["vm_swap_kib"], 128)
        self.assertEqual(memory["vm_hwm_kib"], 20480)
        self.assertEqual(memory["threads"], 5)

    def test_missing_rss_anon_is_an_error_not_a_zero(self):
        with self.assertRaises(mp.MemoryReadError):
            mp.parse_proc_status("Name:\thelper\nVmSwap:\t8 kB\n")


class MemoryMetricTest(unittest.TestCase):
    def test_windows_reads_private_usage_through_ctypes(self):
        if sys.platform != "win32":
            self.skipTest("Windows only")
        memory = mp.read_memory(os.getpid())
        self.assertGreater(memory["private_usage_bytes"], 0)
        self.assertGreaterEqual(memory["working_set_bytes"], 0)
        metric = mp.memory_metric()
        self.assertEqual(metric["name"], "private_usage_bytes")
        self.assertEqual(metric["unit"], "bytes")

    def test_linux_metric_matches_the_parser(self):
        if not sys.platform.startswith("linux"):
            self.skipTest("Linux only")
        memory = mp.read_memory(os.getpid())
        self.assertGreater(memory["rss_anon_kib"], 0)
        self.assertIn("vm_swap_kib", memory)
        metric = mp.memory_metric()
        self.assertEqual(metric["name"], "rss_anon_kib")
        self.assertEqual(metric["unit"], "KiB")


class HelperProcessTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def helper(self, source):
        path = self.root / "helper.py"
        path.write_text(source, encoding="utf-8")
        return [sys.executable, str(path)]

    def test_spawn_first_response_and_synchronous_round_trips(self):
        module = self.root / "fake.wasm"
        module.write_bytes(b"\0asm")
        process = mp.HelperProcess(self.helper(FAKE_HELPER), timeout=10.0)
        self.addCleanup(process.kill)
        process.start()

        first, spawn_ns = process.first_response()
        self.assertEqual(first.kind, mp.Kind.READY)
        self.assertEqual(first.id, 0)
        self.assertGreaterEqual(spawn_ns, 0)
        self.assertLess(spawn_ns, 10_000_000_000)

        process.send(mp.Kind.LOAD, 1, {"type": "load", "plugin": "measure", "module": str(module)})
        self.assertEqual(process.wait_for(1, mp.Kind.LOADED).kind, mp.Kind.LOADED)

        for frame_id in range(2, 6):
            process.send(
                mp.Kind.CALL,
                frame_id,
                {"type": "call", "plugin": "measure", "method": "echo", "payload": {"n": frame_id}},
            )
            reply = process.wait_for(frame_id, mp.Kind.REPLY)
            self.assertEqual(reply.payload_json()["payload"], {"n": frame_id})

        process.send(mp.Kind.UNLOAD, 6, {"type": "unload", "plugin": "measure"})
        self.assertEqual(process.wait_for(6, mp.Kind.UNLOADED).kind, mp.Kind.UNLOADED)

        ending = process.shutdown(timeout=10.0)
        self.assertEqual(ending["exit_code"], 0)
        self.assertTrue(ending["clean_eof"])

    def test_closing_stdin_is_a_clean_end_of_stream(self):
        process = mp.HelperProcess(self.helper(FAKE_HELPER), timeout=10.0)
        self.addCleanup(process.kill)
        process.start()
        process.first_response()
        ending = process.finish(timeout=10.0)
        self.assertEqual(ending["exit_code"], 0)
        self.assertTrue(ending["clean_eof"])

    def test_helper_exiting_before_a_frame_is_reported(self):
        process = mp.HelperProcess([sys.executable, "-c", "raise SystemExit(0)"], timeout=10.0)
        self.addCleanup(process.kill)
        process.start()
        with self.assertRaises(mp.HelperExited) as raised:
            process.first_response(timeout=10.0)
        self.assertEqual(raised.exception.exit_code, 0)

    def test_a_reply_paired_with_another_id_is_a_protocol_violation(self):
        process = mp.HelperProcess(self.helper(WRONG_ID_HELPER), timeout=10.0)
        self.addCleanup(process.kill)
        process.start()
        process.first_response()
        process.send(mp.Kind.CALL, 10, {"type": "call", "plugin": "p", "method": "m"})
        with self.assertRaises(mp.ProtocolViolation):
            process.wait_for(10, mp.Kind.REPLY, timeout=5.0)

    def test_a_response_whose_json_type_contradicts_its_kind_is_refused(self):
        process = mp.HelperProcess(self.helper(WRONG_TYPE_HELPER), timeout=10.0)
        self.addCleanup(process.kill)
        process.start()
        process.first_response()
        process.send(mp.Kind.CALL, 12, {"type": "call", "plugin": "p", "method": "m"})
        with self.assertRaises(mp.ProtocolViolation):
            process.wait_for(12, mp.Kind.REPLY, timeout=5.0)

    def test_a_silent_helper_times_out(self):
        process = mp.HelperProcess(self.helper(SILENT_HELPER), timeout=10.0)
        self.addCleanup(process.kill)
        process.start()
        process.first_response()
        process.send(mp.Kind.CALL, 11, {"type": "call", "plugin": "p", "method": "m"})
        with self.assertRaises(TimeoutError):
            process.wait_for(11, mp.Kind.REPLY, timeout=0.2)


class ReportTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fake = self.root / "helper.py"
        self.fake.write_text(FAKE_HELPER, encoding="utf-8")
        self.module = self.root / "fake.wasm"
        self.module.write_bytes(b"\0asm")

    def measure(self):
        return mp.measure(
            helper=[sys.executable, str(self.fake)],
            module=self.module,
            method="echo",
            plugin="measure",
            requests=4,
            warmup=1,
            payload_bytes=64,
            samples=2,
            interval_ms=10,
            timeout=10.0,
            profile="measure",
            os_tag="unit-test",
            run_label="round-trip",
        )

    def test_report_carries_explicit_platform_metric_and_build_labels(self):
        report = self.measure()
        self.assertEqual(report["schema"], mp.SCHEMA)
        self.assertEqual(report["platform"]["tag"], "unit-test")
        self.assertEqual(report["platform"]["os"], sys.platform)
        self.assertIn("os_version", report["platform"])
        self.assertIn("machine", report["platform"])
        self.assertTrue(report["metric"]["name"])
        self.assertTrue(report["metric"]["unit"])
        self.assertIn("source", report["metric"])
        self.assertEqual(report["build"]["profile"], "measure")
        self.assertRegex(report["build"]["helper_sha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(report["build"]["argv"], [sys.executable, str(self.fake)])

        latency = report["latency"]
        self.assertEqual(latency["clock"], "time.monotonic_ns")
        self.assertEqual(latency["spawn_to_first_response_ns"]["unit"], "nanoseconds")
        self.assertEqual(latency["round_trip_ns"]["unit"], "nanoseconds")
        self.assertEqual(len(latency["round_trip_ns"]["samples"]), 4)
        self.assertGreaterEqual(latency["spawn_to_first_response_ns"]["samples"][0], 0)
        for sample in latency["round_trip_ns"]["samples"]:
            self.assertIsInstance(sample, int)
            self.assertGreater(sample, 0)
        self.assertEqual(set(latency["round_trip_ns"]["percentiles"]), {"p50", "p95", "p99"})

        memory = report["memory"]
        self.assertEqual(memory["metric"], report["metric"]["name"])
        self.assertEqual(memory["unit"], report["metric"]["unit"])
        self.assertGreaterEqual(memory["sample_count"], 2)
        self.assertEqual(report["helper_exit"]["exit_code"], 0)
        self.assertTrue(report["helper_exit"]["clean_eof"])

    def test_a_helper_that_exits_after_the_last_unload_still_reports(self):
        self.fake.write_text(EXIT_ON_UNLOAD_HELPER, encoding="utf-8")
        report = self.measure()
        self.assertEqual(report["helper_exit"]["exit_code"], 0)
        self.assertTrue(report["helper_exit"]["clean_eof"])
        self.assertEqual(len(report["latency"]["round_trip_ns"]["samples"]), 4)

    def test_report_makes_no_budget_verdict(self):
        report = self.measure()
        rendered = json.dumps(report).lower()
        self.assertNotIn("verdict", rendered)
        self.assertNotIn("budget", rendered)
        self.assertNotIn("pass", rendered)


class CommandLineTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fake = self.root / "helper.py"
        self.fake.write_text(FAKE_HELPER, encoding="utf-8")
        self.module = self.root / "fake.wasm"
        self.module.write_bytes(b"\0asm")
        self.output = self.root / "results.json"

    def run_cli(self, *arguments):
        return subprocess.run(
            [sys.executable, str(MODULE_PATH), *arguments],
            capture_output=True,
            text=True,
            timeout=60,
        )

    def test_writes_json_to_the_requested_path(self):
        result = self.run_cli(
            "--helper", sys.executable,
            "--helper-arg", str(self.fake),
            "--module", str(self.module),
            "--requests", "3",
            "--warmup", "0",
            "--payload-bytes", "128",
            "--samples", "2",
            "--interval-ms", "10",
            "--profile", "measure",
            "--os-tag", "cli-test",
            "--json-out", str(self.output),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(self.output.read_text(encoding="utf-8"))
        self.assertEqual(report["platform"]["tag"], "cli-test")
        self.assertEqual(len(report["latency"]["round_trip_ns"]["samples"]), 3)
        self.assertIn("spawn_to_first_response_ns", report["latency"])

    def test_missing_helper_is_a_prerequisite_failure_with_a_reason(self):
        missing = self.root / "no-such-helper"
        result = self.run_cli(
            "--helper", str(missing),
            "--json-out", str(self.output),
        )
        self.assertEqual(result.returncode, 3)
        report = json.loads(self.output.read_text(encoding="utf-8"))
        self.assertEqual(report["error"]["kind"], "prerequisite")
        self.assertIn(str(missing), report["error"]["message"])
        self.assertNotIn("latency", report)
        self.assertNotIn("memory", report)

    def test_a_relative_module_is_refused(self):
        result = self.run_cli(
            "--helper", sys.executable,
            "--helper-arg", str(self.fake),
            "--module", "relative.wasm",
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("absolute", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
