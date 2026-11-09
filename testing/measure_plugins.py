#!/usr/bin/env python3
"""Measure the plugin helper's first call, load times, memory and threads.

The harness spawns a helper binary, speaks the framed JSON protocol from
`docs/plugin-protocol.md`, times the spawn-to-ready handshake, the spawn
through the first guest call (module load included), each per-instance load
and synchronous call round trips with `time.monotonic_ns`, samples the
helper's memory and thread count from the operating system, and writes one
machine-readable JSON record. It judges nothing: there are no budget
constants and no verdicts here, and a run with no data reports that rather
than inventing samples.

Run it against a real helper only after `crates/yapfast-plugin-host` builds:

    python3 testing/measure_plugins.py \
        --helper target/measure/yapfast-plugins \
        --module /absolute/path/to/echo.wasm \
        --method echo --requests 200 --warmup 20 --instances 1 \
        --profile measure --json-out target/plugin-measurements/results.json

See `docs/plugin-measurements.md` for the metric definitions, the JSON shape
and the limits of Python timing.
"""
import argparse
import ctypes
import hashlib
import json
import math
import platform
import queue
import struct
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path
from typing import Any

SCHEMA = "yapfast-plugin-measurement/2"

HEADER_LEN = 4 + 1 + 8
MAX_FRAME_LEN = 1024 * 1024
MAX_PAYLOAD_LEN = MAX_FRAME_LEN - HEADER_LEN

_HEADER = struct.Struct("<IBQ")


class Kind(IntEnum):
    """The frame kind bytes; these are the wire format and must not change."""

    READY = 1
    LOAD = 2
    LOADED = 3
    CALL = 4
    REPLY = 5
    UNLOAD = 6
    UNLOADED = 7
    SHUTDOWN = 8
    ERROR = 9


_RESPONSE_TYPE_NAMES = {
    Kind.READY: "ready",
    Kind.LOADED: "loaded",
    Kind.REPLY: "reply",
    Kind.UNLOADED: "unloaded",
    Kind.ERROR: "error",
}


@dataclass(frozen=True)
class Frame:
    """One frame: a kind, the id pairing it with its counterpart, and payload bytes."""

    kind: Kind
    id: int
    payload: bytes

    def payload_json(self) -> Any:
        """The payload decoded as JSON, or `None` for an empty payload."""
        if not self.payload:
            return None
        return json.loads(self.payload.decode("utf-8"))


class FrameError(ValueError):
    """A payload would make a frame larger than the protocol allows."""


class ReadError(OSError):
    """A stream did not contain a frame: bad length, kind or end of stream."""

    def __init__(
        self,
        message: str,
        *,
        declared: int | None = None,
        kind: int | None = None,
        expected: int | None = None,
        got: int | None = None,
    ) -> None:
        super().__init__(message)
        self.declared = declared
        self.kind = kind
        self.expected = expected
        self.got = got


class ProtocolViolation(RuntimeError):
    """A helper answered with the wrong id or frame kind."""


class HelperError(RuntimeError):
    """The helper answered a request with an error frame, or could not start."""


class PrerequisiteError(RuntimeError):
    """A helper binary or module the measurement needs is missing."""


class HelperExited(RuntimeError):
    """The helper's stdout ended before the frame the harness waited for."""

    def __init__(self, exit_code: int | None, stderr_tail: str = "") -> None:
        super().__init__(f"helper exited with code {exit_code} before replying")
        self.exit_code = exit_code
        self.stderr_tail = stderr_tail


class MemoryReadError(RuntimeError):
    """The operating system would not give a memory sample for the process."""


class ThreadCountError(RuntimeError):
    """The operating system would not give a thread count for the process."""


def encode_frame(kind: Kind, frame_id: int, payload: bytes) -> bytes:
    """Encode one frame: u32 LE payload length, kind byte, u64 LE id, payload."""
    payload = bytes(payload)
    if len(payload) > MAX_PAYLOAD_LEN:
        raise FrameError(
            f"payload of {len(payload)} bytes exceeds the limit of {MAX_PAYLOAD_LEN}"
        )
    return _HEADER.pack(len(payload), int(kind), frame_id) + payload


def read_frame(stream: Any) -> Frame | None:
    """Read one frame, or `None` at a clean end of stream.

    A short read is retried until the requested bytes arrive, because a pipe
    may deliver a header or payload in arbitrary fragments. End of stream
    before the header's first byte is clean; end of stream partway through a
    header or payload is a `ReadError`.
    """
    header = _read_exact(stream, HEADER_LEN, clean_eof=True)
    if header is None:
        return None

    length, kind_byte, frame_id = _HEADER.unpack(header)
    if length > MAX_PAYLOAD_LEN:
        raise ReadError(
            f"frame of {length} payload bytes exceeds the limit of {MAX_PAYLOAD_LEN}",
            declared=length,
        )
    try:
        kind = Kind(kind_byte)
    except ValueError as error:
        raise ReadError(
            f"frame kind {kind_byte} is not one this protocol defines",
            kind=kind_byte,
        ) from error

    payload = _read_exact(stream, length, clean_eof=False)
    assert payload is not None
    return Frame(kind, frame_id, payload)


def _read_exact(stream: Any, count: int, *, clean_eof: bool) -> bytes | None:
    chunks: list[bytes] = []
    got = 0
    while got < count:
        try:
            chunk = stream.read(count - got)
        except InterruptedError:
            continue
        if not chunk:
            if clean_eof and got == 0:
                return None
            raise ReadError(
                f"stream ended after {got} of {count} bytes",
                expected=count,
                got=got,
            )
        chunks.append(chunk)
        got += len(chunk)
    return b"".join(chunks)


def validate_frame_id(kind: Kind, frame_id: int) -> None:
    """Refuse an id the protocol reserves or a frame that misuses id 0.

    Id 0 belongs to the unsolicited `ready` frame alone. A `ready` away from
    id 0 and any other frame at id 0 pair with nothing, so either is a
    violation rather than something to skip.
    """
    if kind is Kind.READY:
        if frame_id != 0:
            raise ProtocolViolation(f"ready carries id {frame_id}, but ready is the frame at id 0")
        return
    if frame_id == 0:
        raise ProtocolViolation(f"{kind.name} carries id 0, which is reserved for ready")


def instance_ids(plugin: str, instances: int) -> list[str]:
    """The plugin ids to load from one module.

    One instance keeps the configured id verbatim, so a single-instance run is
    interchangeable with an earlier one. More than one instance needs distinct
    ids, so they are suffixed `-0`, `-1`, and so on.
    """
    if instances < 1:
        raise ValueError("instances must be at least one")
    if instances == 1:
        return [plugin]
    return [f"{plugin}-{index}" for index in range(instances)]


def nearest_rank(values: list[int], percent: float) -> int:
    """The nearest-rank percentile: sort, then take index ceil(p/100 * n) - 1.

    The result is always one of the observed values, never an interpolation.
    A percent must be above 0 and at most 100; an empty list has no percentile.
    """
    if not 0 < percent <= 100:
        raise ValueError("a percentile must be above 0 and at most 100")
    if not values:
        raise ValueError("an empty sample has no percentile")
    ordered = sorted(values)
    rank = max(1, math.ceil(percent / 100 * len(ordered)))
    return ordered[rank - 1]


def percentiles(values: list[int]) -> dict[str, int]:
    """p50, p95 and p99 by the documented nearest-rank method."""
    return {name: nearest_rank(values, float(name[1:])) for name in ("p50", "p95", "p99")}


def memory_metric() -> dict[str, str]:
    """The metric this platform reports, with its source and comparability."""
    if sys.platform == "win32":
        return {
            "name": "private_usage_bytes",
            "unit": "bytes",
            "source": "GetProcessMemoryInfo(PROCESS_MEMORY_COUNTERS_EX).PrivateUsage",
            "comparison": (
                "Windows private usage is not comparable with Linux RssAnon; "
                "compare a result only with results from the same operating system."
            ),
        }
    if sys.platform.startswith("linux"):
        return {
            "name": "rss_anon_kib",
            "unit": "KiB",
            "source": "/proc/<pid>/status: RssAnon",
            "comparison": (
                "Linux RssAnon is not comparable with Windows private usage; "
                "compare a result only with results from the same operating system. "
                "VmSwap is reported beside it."
            ),
        }
    raise MemoryReadError(f"no memory metric for platform {sys.platform!r}")


def threads_metric(platform_name: str | None = None) -> dict[str, str]:
    """Where a thread count comes from on this platform, and how it compares."""
    name = sys.platform if platform_name is None else platform_name
    if name == "win32":
        return {
            "name": "threads",
            "unit": "threads",
            "source": (
                "Toolhelp32 snapshot: CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0), "
                "then Thread32First/Thread32Next"
            ),
            "comparison": (
                "The count includes every thread the process owns, engine and "
                "compile-pool threads alike; it is not a count of busy threads."
            ),
        }
    if name.startswith("linux"):
        return {
            "name": "threads",
            "unit": "threads",
            "source": "/proc/<pid>/status: Threads",
            "comparison": (
                "The count includes every thread the process owns, engine and "
                "compile-pool threads alike; it is not a count of busy threads."
            ),
        }
    raise ThreadCountError(f"no thread count for platform {name!r}")


def safe_threads_metric() -> dict[str, Any]:
    """`threads_metric`, or a named gap on a platform the harness cannot sample."""
    try:
        return threads_metric()
    except ThreadCountError as error:
        return {"name": None, "unit": None, "source": None, "comparison": None, "error": str(error)}


def safe_memory_metric() -> dict[str, Any]:
    """`memory_metric`, or a named gap on a platform the harness cannot sample."""
    try:
        return memory_metric()
    except MemoryReadError as error:
        return {"name": None, "unit": None, "source": None, "comparison": None, "error": str(error)}


def parse_proc_status(text: str) -> dict[str, int]:
    """Parse the fields read from `/proc/<pid>/status`, in KiB and thread count."""
    fields = {
        "RssAnon": "rss_anon_kib",
        "VmSwap": "vm_swap_kib",
        "VmHWM": "vm_hwm_kib",
        "Threads": "threads",
    }
    memory: dict[str, int] = {}
    for line in text.splitlines():
        key, separator, value = line.partition(":")
        if not separator or key not in fields:
            continue
        parts = value.split()
        if parts:
            memory[fields[key]] = int(parts[0])

    missing = [name for name in ("rss_anon_kib", "vm_swap_kib") if name not in memory]
    if missing:
        raise MemoryReadError(f"/proc status has no {', '.join(missing)}")
    return memory


def read_memory(pid: int) -> dict[str, int]:
    """Sample one process's memory in the metric this platform reports."""
    if sys.platform.startswith("linux"):
        try:
            text = Path(f"/proc/{pid}/status").read_text(encoding="utf-8", errors="replace")
        except OSError as error:
            raise MemoryReadError(f"cannot read /proc/{pid}/status: {error}") from error
        return parse_proc_status(text)
    if sys.platform == "win32":
        return _read_windows_memory(pid)
    raise MemoryReadError(f"no memory sampler for platform {sys.platform!r}")


class _ProcessMemoryCountersEx(ctypes.Structure):
    _fields_ = [
        ("cb", ctypes.c_ulong),
        ("PageFaultCount", ctypes.c_ulong),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


_WINDOWS_MEMORY_API: dict[str, Any] | None = None


def _windows_memory_api() -> dict[str, Any]:
    global _WINDOWS_MEMORY_API
    if _WINDOWS_MEMORY_API is None:
        from ctypes import wintypes

        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

        psapi.GetProcessMemoryInfo.argtypes = [
            wintypes.HANDLE,
            ctypes.POINTER(_ProcessMemoryCountersEx),
            wintypes.DWORD,
        ]
        psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
        kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel32.OpenProcess.restype = wintypes.HANDLE
        kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel32.CloseHandle.restype = wintypes.BOOL

        _WINDOWS_MEMORY_API = {
            "GetProcessMemoryInfo": psapi.GetProcessMemoryInfo,
            "OpenProcess": kernel32.OpenProcess,
            "CloseHandle": kernel32.CloseHandle,
        }
    return _WINDOWS_MEMORY_API


def _read_windows_memory(pid: int) -> dict[str, int]:
    api = _windows_memory_api()
    # PROCESS_QUERY_INFORMATION | PROCESS_VM_READ is what GetProcessMemoryInfo needs.
    handle = api["OpenProcess"](0x0400 | 0x0010, False, pid)
    if not handle:
        raise MemoryReadError(
            f"OpenProcess({pid}) failed: {ctypes.WinError(ctypes.get_last_error())}"
        )
    try:
        counters = _ProcessMemoryCountersEx()
        counters.cb = ctypes.sizeof(counters)
        if not api["GetProcessMemoryInfo"](handle, ctypes.byref(counters), counters.cb):
            raise MemoryReadError(
                f"GetProcessMemoryInfo({pid}) failed: {ctypes.WinError(ctypes.get_last_error())}"
            )
        return {
            "private_usage_bytes": int(counters.PrivateUsage),
            "working_set_bytes": int(counters.WorkingSetSize),
            "peak_working_set_bytes": int(counters.PeakWorkingSetSize),
        }
    finally:
        api["CloseHandle"](handle)


TH32CS_SNAPTHREAD = 0x00000004
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value or -1


class _ThreadEntry32(ctypes.Structure):
    _fields_ = [
        ("dwSize", ctypes.c_ulong),
        ("cntUsage", ctypes.c_ulong),
        ("th32ThreadID", ctypes.c_ulong),
        ("th32OwnerProcessID", ctypes.c_ulong),
        ("tpBasePri", ctypes.c_long),
        ("tpDeltaPri", ctypes.c_long),
        ("dwFlags", ctypes.c_ulong),
    ]


_WINDOWS_THREAD_API: dict[str, Any] | None = None


def _windows_thread_api() -> dict[str, Any]:
    """The Win32 thread-snapshot calls, with the structure marshalling done here.

    The dictionary is the boundary tests replace: its `Thread32First` and
    `Thread32Next` take the entry object itself, so a fake need not build
    ctypes pointers.
    """
    global _WINDOWS_THREAD_API
    if _WINDOWS_THREAD_API is None:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateToolhelp32Snapshot.argtypes = [ctypes.c_ulong, ctypes.c_ulong]
        kernel32.CreateToolhelp32Snapshot.restype = ctypes.c_void_p
        kernel32.Thread32First.argtypes = [ctypes.c_void_p, ctypes.POINTER(_ThreadEntry32)]
        kernel32.Thread32First.restype = ctypes.c_int
        kernel32.Thread32Next.argtypes = [ctypes.c_void_p, ctypes.POINTER(_ThreadEntry32)]
        kernel32.Thread32Next.restype = ctypes.c_int
        kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
        kernel32.CloseHandle.restype = ctypes.c_int

        def first(handle: Any, entry: _ThreadEntry32) -> bool:
            return bool(kernel32.Thread32First(handle, ctypes.byref(entry)))

        def next_entry(handle: Any, entry: _ThreadEntry32) -> bool:
            return bool(kernel32.Thread32Next(handle, ctypes.byref(entry)))

        _WINDOWS_THREAD_API = {
            "CreateToolhelp32Snapshot": kernel32.CreateToolhelp32Snapshot,
            "ThreadEntry": _ThreadEntry32,
            "Thread32First": first,
            "Thread32Next": next_entry,
            "CloseHandle": kernel32.CloseHandle,
            "last_error": ctypes.get_last_error,
            "win_error": ctypes.WinError,
        }
    return _WINDOWS_THREAD_API


def thread_count_from_toolhelp(pid: int, api: dict[str, Any]) -> int:
    """Count one process's threads from a Toolhelp32 snapshot.

    A refused snapshot, and a snapshot that lists no thread for the process,
    are errors: zero would read like a measurement, and a live process always
    has at least one thread.
    """
    snapshot = api["CreateToolhelp32Snapshot"](TH32CS_SNAPTHREAD, 0)
    if snapshot in (0, None, INVALID_HANDLE_VALUE, -1):
        raise ThreadCountError(f"CreateToolhelp32Snapshot failed: {api['win_error']()}")
    try:
        entry = api["ThreadEntry"]()
        if isinstance(entry, ctypes.Structure):
            entry.dwSize = ctypes.sizeof(entry)
        count = 0
        more = api["Thread32First"](snapshot, entry)
        while more:
            if int(entry.th32OwnerProcessID) == pid:
                count += 1
            more = api["Thread32Next"](snapshot, entry)
        if count == 0:
            raise ThreadCountError(
                f"the thread snapshot lists no thread owned by process {pid} "
                f"(GetLastError {api['last_error']()})"
            )
        return count
    finally:
        api["CloseHandle"](snapshot)


def read_thread_count(pid: int, platform_name: str | None = None) -> int:
    """One process's thread count in the metric this platform reports."""
    name = sys.platform if platform_name is None else platform_name
    if name.startswith("linux"):
        try:
            text = Path(f"/proc/{pid}/status").read_text(encoding="utf-8", errors="replace")
        except OSError as error:
            raise ThreadCountError(f"cannot read /proc/{pid}/status: {error}") from error
        for line in text.splitlines():
            key, separator, value = line.partition(":")
            if separator and key == "Threads":
                parts = value.split()
                if parts:
                    return int(parts[0])
        raise ThreadCountError(f"/proc/{pid}/status has no Threads field")
    if name == "win32":
        return thread_count_from_toolhelp(pid, _windows_thread_api())
    raise ThreadCountError(f"no thread count for platform {name!r}")


class MemorySampler:
    """Samples one live process on a background thread.

    Memory is the platform's metric. A thread count is sampled beside it when
    the platform's reader does not already return one, as Linux's `/proc`
    status does; a thread-count failure is reported beside the samples rather
    than turned into a zero.
    """

    def __init__(
        self,
        pid: int,
        interval_ms: int,
        reader=read_memory,
        thread_reader=read_thread_count,
    ) -> None:
        self.pid = pid
        self.interval = interval_ms / 1000
        self.reader = reader
        self.thread_reader = thread_reader
        self.samples: list[dict[str, int]] = []
        self.errors: list[str] = []
        self.thread_errors: list[str] = []
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        self._thread = threading.Thread(target=self._run, name="plugin-memory-sampler", daemon=True)
        self._thread.start()

    def _run(self) -> None:
        while not self._stop.is_set():
            try:
                sample = self.reader(self.pid)
            except MemoryReadError as error:
                self.errors.append(str(error))
                return
            entry = {"t_ns": time.monotonic_ns(), **sample}
            if "threads" not in entry and self.thread_reader is not None:
                try:
                    entry["threads"] = self.thread_reader(self.pid)
                except ThreadCountError as error:
                    self.thread_errors.append(str(error))
            self.samples.append(entry)
            self._stop.wait(self.interval)

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5.0)

    def peak(self) -> dict[str, int] | None:
        if not self.samples:
            return None
        keys = {key for sample in self.samples for key in sample if key != "t_ns"}
        return {key: max(sample[key] for sample in self.samples if key in sample) for key in sorted(keys)}


class HelperProcess:
    """A child speaking the framed JSON protocol over stdin and stdout."""

    def __init__(self, argv: list[str], timeout: float = 10.0) -> None:
        self.argv = [str(argument) for argument in argv]
        self.timeout = timeout
        self.process: subprocess.Popen[bytes] | None = None
        self._spawn_ns = 0
        self._queue: queue.Queue[tuple[str, Any]] = queue.Queue()
        self._reader: threading.Thread | None = None
        self._stderr_reader: threading.Thread | None = None
        self._stderr_tail = b""
        self._saw_eof = False
        self._read_error: BaseException | None = None

    @property
    def pid(self) -> int:
        if self.process is None:
            raise HelperError("the helper has not been started")
        return self.process.pid

    def start(self) -> None:
        self._spawn_ns = time.monotonic_ns()
        try:
            self.process = subprocess.Popen(
                self.argv,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                bufsize=0,
            )
        except OSError as error:
            raise HelperError(f"cannot start {self.argv[0]}: {error}") from error
        self._reader = threading.Thread(target=self._read_frames, name="plugin-frame-reader", daemon=True)
        self._stderr_reader = threading.Thread(target=self._drain_stderr, name="plugin-stderr", daemon=True)
        self._reader.start()
        self._stderr_reader.start()

    def _read_frames(self) -> None:
        assert self.process is not None and self.process.stdout is not None
        try:
            while True:
                frame = read_frame(self.process.stdout)
                if frame is None:
                    self._saw_eof = True
                    self._queue.put(("eof", None))
                    return
                self._queue.put(("frame", frame))
        except BaseException as error:  # the reader reports every failure to its caller
            self._read_error = error
            self._queue.put(("error", error))

    def _drain_stderr(self) -> None:
        assert self.process is not None and self.process.stderr is not None
        tail = bytearray()
        while True:
            try:
                chunk = self.process.stderr.read(4096)
            except (OSError, ValueError):
                self._stderr_tail = bytes(tail)
                return
            if not chunk:
                self._stderr_tail = bytes(tail)
                return
            tail.extend(chunk)
            if len(tail) > 8192:
                del tail[:-8192]

    def stderr_tail(self) -> str:
        return self._stderr_tail.decode("utf-8", errors="replace")

    def _next(self, timeout: float | None = None) -> Frame:
        remaining = self.timeout if timeout is None else timeout
        try:
            event, value = self._queue.get(timeout=max(remaining, 0.0))
        except queue.Empty as error:
            raise TimeoutError(f"no helper frame within {remaining:.3f} s") from error
        if event == "frame":
            return value
        if event == "error":
            raise value
        exit_code = self.process.poll() if self.process is not None else None
        if exit_code is None and self.process is not None:
            try:
                exit_code = self.process.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                pass
        raise HelperExited(exit_code, self.stderr_tail())

    def elapsed_ns(self) -> int:
        """Nanoseconds since the spawn, for a sample taken after some exchange."""
        if self.process is None:
            raise HelperError("the helper has not been started")
        return time.monotonic_ns() - self._spawn_ns

    def first_response(self, timeout: float | None = None) -> tuple[Frame, int]:
        """The `ready` frame, and ns from just before spawn to it.

        `ready` is the handshake: it is the first frame, it carries id 0, and
        anything else is a peer speaking something this harness cannot pair
        requests with.
        """
        frame = self._next(timeout)
        elapsed = time.monotonic_ns() - self._spawn_ns
        if frame.kind is not Kind.READY:
            raise ProtocolViolation(
                f"expected ready as the first frame, got {frame.kind.name}"
            )
        validate_frame_id(frame.kind, frame.id)
        return frame, elapsed

    def send(self, kind: Kind, frame_id: int, body: Any) -> None:
        if self.process is None or self.process.stdin is None:
            raise HelperError("the helper has not been started")
        payload = b"" if body is None else json.dumps(body, separators=(",", ":")).encode("utf-8")
        try:
            self.process.stdin.write(encode_frame(kind, frame_id, payload))
            self.process.stdin.flush()
        except (BrokenPipeError, OSError) as error:
            raise HelperExited(self.process.poll(), self.stderr_tail()) from error

    def wait_for(self, frame_id: int, kind: Kind, timeout: float | None = None) -> Frame:
        """Wait for the response with `frame_id`, skipping an unsolicited `ready`."""
        deadline = time.monotonic() + (self.timeout if timeout is None else timeout)
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"no response with id {frame_id} before the deadline")
            frame = self._next(remaining)
            if frame.kind is Kind.READY:
                validate_frame_id(frame.kind, frame.id)
                continue
            validate_frame_id(frame.kind, frame.id)
            if frame.id != frame_id:
                raise ProtocolViolation(
                    f"expected a frame with id {frame_id}, got id {frame.id} ({frame.kind.name})"
                )
            if frame.kind is Kind.ERROR and kind is not Kind.ERROR:
                message = "the helper reported an error"
                body = frame.payload_json()
                if isinstance(body, dict) and isinstance(body.get("message"), str):
                    message = body["message"]
                raise HelperError(message)
            if frame.kind is not kind:
                raise ProtocolViolation(
                    f"expected {kind.name} for id {frame_id}, got {frame.kind.name}"
                )
            body = frame.payload_json()
            expected_type = _RESPONSE_TYPE_NAMES[frame.kind]
            if not isinstance(body, dict) or body.get("type") != expected_type:
                raise ProtocolViolation(
                    f"{frame.kind.name} frame {frame_id} carries JSON type "
                    f"{body.get('type') if isinstance(body, dict) else type(body).__name__!r}, "
                    f"not {expected_type!r}"
                )
            return frame

    def request(self, kind: Kind, frame_id: int, body: Any, expect: Kind, timeout: float | None = None) -> Frame:
        self.send(kind, frame_id, body)
        return self.wait_for(frame_id, expect, timeout)

    def close_stdin(self) -> None:
        if self.process is not None and self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except OSError:
                pass

    def finish(self, timeout: float | None = None) -> dict[str, Any]:
        """Close stdin, wait for the helper, and report whether stdout ended cleanly."""
        if self.process is None:
            raise HelperError("the helper has not been started")
        self.close_stdin()
        limit = self.timeout if timeout is None else timeout
        try:
            exit_code = self.process.wait(timeout=limit)
        except subprocess.TimeoutExpired as error:
            raise HelperError(f"helper did not exit within {limit:.3f} s") from error
        if self._reader is not None:
            self._reader.join(timeout=2.0)
        if self._stderr_reader is not None:
            self._stderr_reader.join(timeout=1.0)
        return {
            "exit_code": exit_code,
            "clean_eof": self._saw_eof and self._read_error is None,
            "stderr_tail": self.stderr_tail(),
        }

    def shutdown(self, timeout: float | None = None, frame_id: int = 2**64 - 1) -> dict[str, Any]:
        """Ask the helper to exit, then wait for its stdout to end."""
        self.send(Kind.SHUTDOWN, frame_id, {"type": "shutdown"})
        return self.finish(timeout)

    def kill(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.kill()
            try:
                self.process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                pass
        self.close_stdin()
        if self.process.stdout is not None:
            try:
                self.process.stdout.close()
            except OSError:
                pass
        if self.process.stderr is not None:
            try:
                self.process.stderr.close()
            except OSError:
                pass
        if self._reader is not None:
            self._reader.join(timeout=2.0)
        if self._stderr_reader is not None:
            self._stderr_reader.join(timeout=2.0)


def platform_labels(os_tag: str | None = None) -> dict[str, Any]:
    """The operating system labels every report carries."""
    return {
        "os": sys.platform,
        "os_version": platform.version(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "tag": os_tag,
    }


def sha256_file(path: Path) -> str | None:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError:
        return None


def build_labels(argv: list[str], profile: str | None) -> dict[str, Any]:
    """What binary and arguments produced a report, so runs are distinguishable."""
    helper = Path(argv[0])
    exists = helper.is_file()
    return {
        "helper": str(helper.resolve()) if exists else str(helper),
        "helper_sha256": sha256_file(helper) if exists else None,
        "helper_size_bytes": helper.stat().st_size if exists else None,
        "argv": list(argv),
        "profile": profile,
    }


def timing(values: list[int], definition: str) -> dict[str, Any]:
    """One latency block: the definition, the observed samples and percentiles."""
    return {
        "unit": "nanoseconds",
        "definition": definition,
        "samples": list(values),
        "percentiles": percentiles(values) if values else None,
    }


def measure(
    *,
    helper: list[str],
    module: Path | str | None,
    method: str = "echo",
    plugin: str = "measure",
    instances: int = 1,
    requests: int = 200,
    warmup: int = 20,
    payload_bytes: int = 1024,
    samples: int = 10,
    interval_ms: int = 50,
    timeout: float = 10.0,
    profile: str | None = None,
    os_tag: str | None = None,
    run_label: str | None = None,
) -> dict[str, Any]:
    """Run one spawn, load, round-trip and memory measurement, without judging it.

    With a module, the measurement covers the spawn through the first call's
    reply, so module load is inside it. `--instances` loads the module under
    that many distinct plugin ids, samples memory once every load has answered,
    and calls round-robin over the instances before unloading each one.
    """
    argv = [str(argument) for argument in helper]
    module_path: Path | None = None
    if module is not None:
        module_path = Path(module)
        if not module_path.is_absolute():
            raise PrerequisiteError("the module path must be absolute")
        if not module_path.is_file():
            raise PrerequisiteError(f"module not found: {module_path}")

    plugin_ids = instance_ids(plugin, instances)
    thread_metric = safe_threads_metric()
    thread_reader = read_thread_count if thread_metric.get("source") else None

    process = HelperProcess(argv, timeout=timeout)
    sampler: MemorySampler | None = None
    round_trips: list[int] = []
    load_ns: dict[str, int] = {}
    load_completed_ns: int | None = None
    spawn_to_first_call_ns: int | None = None
    try:
        process.start()
        _, spawn_ns = process.first_response(timeout=timeout)

        next_id = 1
        if module_path is not None:
            for plugin_id in plugin_ids:
                started = time.monotonic_ns()
                process.request(
                    Kind.LOAD,
                    next_id,
                    {"type": "load", "plugin": plugin_id, "module": str(module_path)},
                    Kind.LOADED,
                    timeout=timeout,
                )
                load_ns[plugin_id] = time.monotonic_ns() - started
                next_id += 1
            load_completed_ns = time.monotonic_ns()

        sampler = MemorySampler(process.pid, interval_ms, thread_reader=thread_reader)
        sampler.start()

        if module_path is not None:
            calls = 0
            for _ in range(warmup + requests):
                plugin_id = plugin_ids[calls % len(plugin_ids)]
                call = {
                    "type": "call",
                    "plugin": plugin_id,
                    "method": method,
                    "payload": {"data": "x" * payload_bytes},
                }
                measured = calls >= warmup
                started = time.monotonic_ns() if measured else 0
                process.request(Kind.CALL, next_id, call, Kind.REPLY, timeout=timeout)
                if spawn_to_first_call_ns is None:
                    spawn_to_first_call_ns = process.elapsed_ns()
                if measured:
                    round_trips.append(time.monotonic_ns() - started)
                calls += 1
                next_id += 1

        # Let the sampler collect the requested count while the helper is still
        # loaded. Sampling is time-based, so a fast call loop alone may not
        # produce more than the first sample.
        deadline = time.monotonic() + samples * interval_ms / 1000 + 2.0
        while (
            len(sampler.samples) < samples
            and not sampler.errors
            and time.monotonic() < deadline
        ):
            time.sleep(0.005)

        sampler.stop()
        memory_samples = list(sampler.samples)
        memory_errors = list(sampler.errors)
        thread_errors = list(sampler.thread_errors)
        memory_peak = sampler.peak()
        sampler = None

        if module_path is not None:
            for plugin_id in plugin_ids:
                process.request(
                    Kind.UNLOAD,
                    next_id,
                    {"type": "unload", "plugin": plugin_id},
                    Kind.UNLOADED,
                    timeout=timeout,
                )
                next_id += 1

        try:
            ending = process.shutdown(timeout=timeout, frame_id=next_id)
        except HelperExited:
            # A helper may exit as soon as its last instance is unloaded, before a
            # shutdown frame can be written. Its measured data is still valid.
            ending = process.finish(timeout=timeout)
    finally:
        if sampler is not None:
            sampler.stop()
        process.kill()

    thread_values = [sample["threads"] for sample in memory_samples if "threads" in sample]
    loaded = module_path is not None
    return {
        "schema": SCHEMA,
        "platform": platform_labels(os_tag),
        "metric": memory_metric(),
        "build": build_labels(argv, profile),
        "scenario": {
            "name": run_label,
            "module": str(module_path) if loaded else None,
            "method": method if loaded else None,
            "plugin": plugin if loaded else None,
            "instances": len(plugin_ids) if loaded else 0,
            "plugin_ids": plugin_ids if loaded else [],
            "requests": requests if loaded else 0,
            "warmup": warmup if loaded else 0,
            "payload_bytes": payload_bytes,
            "samples": samples,
            "interval_ms": interval_ms,
        },
        "latency": {
            "clock": "time.monotonic_ns",
            "spawn_to_ready_ns": timing(
                [spawn_ns],
                "from immediately before spawn to the ready frame at id 0",
            ),
            "spawn_to_first_call_ns": timing(
                [spawn_to_first_call_ns] if spawn_to_first_call_ns is not None else [],
                "from immediately before spawn through ready, every load, and the "
                "first call's reply",
            ),
            "load_ns": {
                "unit": "nanoseconds",
                "definition": (
                    "around one load request: write the load frame and read the "
                    "loaded frame carrying that id"
                ),
                "samples": list(load_ns.values()),
                "percentiles": percentiles(list(load_ns.values())) if load_ns else None,
                "by_plugin": load_ns,
                "completed_ns": load_completed_ns,
            },
            "round_trip_ns": timing(
                round_trips,
                "write a call frame, flush it, and read the reply frame with its id",
            ),
        },
        "memory": {
            "metric": memory_metric()["name"],
            "unit": memory_metric()["unit"],
            "samples": memory_samples,
            "sample_count": len(memory_samples),
            "peak": memory_peak,
            "errors": memory_errors,
            "first_sample_ns": memory_samples[0]["t_ns"] if memory_samples else None,
            "threads": {
                "unit": thread_metric["unit"],
                "source": thread_metric["source"],
                "samples": thread_values,
                "peak": max(thread_values) if thread_values else None,
                "errors": thread_errors,
                "error": thread_metric.get("error"),
            },
        },
        "helper_exit": ending,
    }


def report_base(argv: list[str], profile: str | None, os_tag: str | None) -> dict[str, Any]:
    return {
        "schema": SCHEMA,
        "platform": platform_labels(os_tag),
        "metric": safe_memory_metric(),
        "build": build_labels(argv, profile),
    }


def _positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return number


def _non_negative_int(value: str) -> int:
    number = int(value)
    if number < 0:
        raise argparse.ArgumentTypeError("must not be negative")
    return number


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--helper", required=True, help="the helper binary to spawn")
    parser.add_argument(
        "--helper-arg",
        action="append",
        default=[],
        metavar="ARG",
        help="an argument to the helper; repeat for each one",
    )
    parser.add_argument("--module", help="an absolute .wasm path to load before calling")
    parser.add_argument("--method", default="echo", help="the method each call names")
    parser.add_argument("--plugin", default="measure", help="the plugin id used on the wire")
    parser.add_argument(
        "--instances",
        type=_positive_int,
        default=1,
        help=(
            "load the module under this many plugin ids; 1 keeps `--plugin` "
            "verbatim, more use `<plugin>-0` .. `<plugin>-(N-1)`"
        ),
    )
    parser.add_argument("--requests", type=_positive_int, default=200, help="measured round trips")
    parser.add_argument("--warmup", type=_non_negative_int, default=20, help="uncounted round trips")
    parser.add_argument(
        "--payload-bytes",
        type=_non_negative_int,
        default=1024,
        help="bytes in the call payload's `data` string",
    )
    parser.add_argument("--samples", type=_positive_int, default=10, help="memory samples")
    parser.add_argument("--interval-ms", type=_positive_int, default=50, help="milliseconds between samples")
    parser.add_argument("--timeout", type=float, default=10.0, help="seconds to wait for any frame")
    parser.add_argument("--profile", help="the build profile the helper was built with")
    parser.add_argument("--os-tag", help="a label for the host, such as `wsl2` or a machine name")
    parser.add_argument("--run-label", help="a label for this run's scenario")
    parser.add_argument("--json-out", type=Path, help="write the report here instead of stdout")
    return parser


def _write_report(report: dict[str, Any], output: Path | None) -> None:
    text = json.dumps(report, indent=2, sort_keys=False) + "\n"
    if output is None:
        sys.stdout.write(text)
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(text, encoding="utf-8")
    sys.stderr.write(f"measure_plugins: wrote {output}\n")


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    arguments = parser.parse_args(argv)

    module = Path(arguments.module) if arguments.module is not None else None
    if module is not None and not module.is_absolute():
        parser.error("--module must be an absolute path")

    helper = [arguments.helper, *arguments.helper_arg]
    base = report_base(helper, arguments.profile, arguments.os_tag)

    try:
        if not Path(arguments.helper).is_file():
            raise PrerequisiteError(f"helper not found: {arguments.helper}")
        if module is not None and not module.is_file():
            raise PrerequisiteError(f"module not found: {module}")
        report = measure(
            helper=helper,
            module=module,
            method=arguments.method,
            plugin=arguments.plugin,
            instances=arguments.instances,
            requests=arguments.requests,
            warmup=arguments.warmup,
            payload_bytes=arguments.payload_bytes,
            samples=arguments.samples,
            interval_ms=arguments.interval_ms,
            timeout=arguments.timeout,
            profile=arguments.profile,
            os_tag=arguments.os_tag,
            run_label=arguments.run_label,
        )
    except PrerequisiteError as error:
        base["error"] = {"kind": "prerequisite", "message": str(error)}
        _write_report(base, arguments.json_out)
        return 3
    except (
        FrameError,
        ReadError,
        ProtocolViolation,
        HelperError,
        HelperExited,
        MemoryReadError,
        TimeoutError,
        OSError,
    ) as error:
        base["error"] = {"kind": "measurement", "message": str(error)}
        _write_report(base, arguments.json_out)
        return 1

    _write_report(report, arguments.json_out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
