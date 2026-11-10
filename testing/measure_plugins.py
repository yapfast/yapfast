#!/usr/bin/env python3
"""Measure the memory the plugin budgets turn on.

Three things decide those budgets: what the helper costs while it holds a
plugin, what each further plugin adds, and what the parent pays when the plugin
system is off. This measures all three, plus the two ways the cost is supposed
to go away again: the helper exits when the last instance is evicted, and the
parent is back at its earlier level with plugins off.

The helper half drives `yapfast-plugins run` over its pipe, with the frames in
`yapfast_plugin_api::frame`: the same binary the parent starts, with nothing
stubbed on the way in. Every plugin it holds is the module given with
`--plugin`, loaded once per plugin id. The parent half installs `--plugin-dir`
with `yapfast-plugins fetch`, starts `--parent` with that data directory, and
samples the parent and the helper it spawns.

Memory is `RssAnon` from `/proc/PID/status`: the private anonymous memory, which
is what a plugin system's cost is, without the mapped code and fonts that fall
whenever the kernel reclaims them. The parent also reports its helper's share
and the two together. Threads are counted from `/proc/PID/task`, because the
point of the scheduler is that the plugin count doesn't move them.

One line per measurement, with the spread across runs. The budgets are read by
a person, so this exits 0 whenever every measurement was taken, whatever the
numbers say; it exits non-zero only when one could not be.

    nix develop path:./nix --command cargo build --profile measure --bin yapfast
    nix develop path:./nix --command cargo build --profile measure -p yapfast-plugin-host
    testing/measure_plugins.py \\
        --parent target/measure/yapfast \\
        --helper target/measure/yapfast-plugins \\
        --plugin testing/plugins/echo.wasm \\
        --plugin-dir testing/plugins/example-plugin
"""
import argparse
import contextlib
import json
import os
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parent.parent

# One frame of the helper's pipe: payload length, kind, id, then the payload.
HEADER = struct.Struct('<IBQ')
HELLO, REQUEST, REPLY, EVENT, REPORT, LOG, BYE = 1, 2, 3, 4, 5, 6, 7


class Failed(Exception):
    """A measurement couldn't be taken; the reason says what blocked it."""


def frame(kind, id, message):
    payload = b'' if message is None else json.dumps(message).encode()
    return HEADER.pack(len(payload), kind, id) + payload


class Helper:
    """`yapfast-plugins run`, on its own pipe."""

    def __init__(self, binary, cache, log_path):
        self.process = subprocess.Popen(
            [str(binary), 'run', '--cache', str(cache)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=log_path.open('wb') if log_path else subprocess.DEVNULL)
        if self.process.stdin is None or self.process.stdout is None:
            raise Failed('the helper started without a pipe')
        self.stdin = self.process.stdin
        self.stdout = self.process.stdout
        self.next = 1

    def send(self, kind, id, message):
        self.stdin.write(frame(kind, id, message))
        self.stdin.flush()

    def recv(self, timeout):
        """One frame, or None when nothing arrived within `timeout`."""
        ready, _, _ = select.select([self.stdout], [], [], timeout)
        if not ready:
            return None
        header = self.stdout.read(HEADER.size)
        if len(header) < HEADER.size:
            return 'closed'
        length, kind, id = HEADER.unpack(header)
        payload = self.stdout.read(length) if length else b''
        if len(payload) < length:
            return 'closed'
        if not payload:
            return kind, id, None
        try:
            return kind, id, json.loads(payload)
        except ValueError as error:
            raise Failed(
                f'the helper sent a {kind} frame that is not JSON: {error}'
            ) from error

    def wait(self, wanted, timeout, what):
        """The first message `wanted` accepts, skipping what it doesn't."""
        deadline = time.monotonic() + timeout
        while True:
            left = deadline - time.monotonic()
            if left <= 0:
                raise Failed(f'the helper never said {what} within {timeout:.0f} s')
            got = self.recv(left)
            if got == 'closed':
                raise Failed(f'the helper closed its pipe before saying {what}')
            if got is None:
                continue
            _, _, message = got
            if not isinstance(message, dict):
                continue
            if wanted(message):
                return message
            if message.get('t') == 'fault':
                raise Failed(f'the helper faulted on {what}: '
                             f'{message["fault"]["kind"]}: {message["fault"]["reason"]}')

    def hello(self):
        self.send(HELLO, 0, {'t': 'hello', 'api': 1})
        self.wait(lambda m: m.get('t') == 'ready', 60, 'Ready')

    def activate(self, plugin, wasm, pages):
        """Loads and activates one plugin, and waits until it is up."""
        self.send(REQUEST, 0, {'t': 'load', 'plugin': plugin, 'wasm': str(wasm),
                               'memory_pages': pages, 'wasi': False})
        self.next += 1
        id = self.next
        self.send(REQUEST, id, {'t': 'activate', 'plugin': plugin})
        # The answer to the request, or the status report that says the same
        # thing: which of the two a helper sends is its business, and both mean
        # the instance holds its memory by the time this returns.
        self.wait(lambda m: m.get('t') == 'activated'
                  or (m.get('t') == 'status' and m.get('status') == 'active'),
                  120, f'Activated for {plugin}')

    def evict(self, plugin):
        self.send(REQUEST, 0, {'t': 'evict', 'plugin': plugin})

    def close(self):
        """Stops the helper, and says whether it had already gone."""
        if self.process.poll() is not None:
            return True
        with contextlib.suppress(OSError):
            self.stdin.close()
        try:
            self.process.wait(10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(10)
        return True

    def gone(self, timeout):
        """Whether the helper exited by itself, as it does with nothing left."""
        try:
            self.process.wait(timeout)
            return True
        except subprocess.TimeoutExpired:
            return False


def anonymous_mib(pid):
    """The private anonymous memory of a process, in MiB."""
    try:
        status = Path(f'/proc/{pid}/status').read_text().splitlines()
    except OSError as error:
        raise Failed(f'the process {pid} went away: {error}') from error
    for line in status:
        if line.startswith('RssAnon:'):
            try:
                return int(line.split()[1]) / 1024
            except (IndexError, ValueError):
                break
    raise Failed(f'/proc/{pid}/status has no RssAnon on this kernel')


def threads(pid):
    try:
        return len(os.listdir(f'/proc/{pid}/task'))
    except OSError as error:
        raise Failed(f'the process {pid} went away: {error}') from error


def helper_run(args, run, cache, log_path):
    """One helper's lifecycle, sampled at each of its stages."""
    helper = Helper(args.helper, cache, log_path / f'helper-{run}.log')
    samples = {}
    try:
        helper.hello()
        time.sleep(args.settle)
        samples['at rest'] = sample(helper.process.pid)
        plugins = [f'com.example.measure{i}' for i in range(args.plugins)]
        helper.activate(plugins[0], args.plugin, args.pages)
        time.sleep(args.settle)
        samples['one plugin'] = sample(helper.process.pid)
        for plugin in plugins[1:]:
            helper.activate(plugin, args.plugin, args.pages)
        time.sleep(args.settle)
        samples['many plugins'] = sample(helper.process.pid)
        for plugin in plugins:
            helper.evict(plugin)
        samples['exited'] = {'gone': helper.gone(args.timeout)}
    finally:
        helper.close()
    return samples


def sample(pid):
    return {'rss': anonymous_mib(pid), 'threads': threads(pid)}


def parent_run(args, run, data, off, log_path):
    """One parent, with the plugin system on or off, sampled once settled."""
    command = [str(args.parent), '--data-dir', str(data), *args.parent_arg]
    if off:
        command.append('--no-plugins')
    log = (log_path / f'parent-{run}-{"off" if off else "on"}.log').open('wb')
    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
    try:
        helpers = []
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise Failed('the parent exited before it settled; see the log in '
                             f'{log_path}')
            helpers = [pid for pid in children(process.pid)
                       if b'yapfast-plugins' in Path(f'/proc/{pid}/cmdline').read_bytes()]
            if helpers or off:
                break
            time.sleep(.1)
        if not off and not helpers:
            raise Failed('the parent started no helper, so no plugin activated: '
                         f'check that {args.plugin_dir} installed and is enabled')
        time.sleep(args.settle)
        parent = sample(process.pid)
        parent['helpers'] = [sample(pid) for pid in helpers]
        return parent
    finally:
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(20)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(20)
        log.close()


def children(pid):
    try:
        return [int(entry) for entry in Path(f'/proc/{pid}/task/{pid}/children')
                .read_text().split()]
    except (FileNotFoundError, PermissionError):
        return []


def fields(values, pick):
    return [pick(value) for value in values]


def spread(values):
    if not values:
        return 'nothing measured'
    low, high = min(values), max(values)
    return f'{low:.2f}-{high:.2f}' if abs(high - low) >= 0.005 else f'{low:.2f}'


def emit(label, values, unit='MiB'):
    print(f'{label}: {sum(values) / len(values):.2f} {unit} ({spread(values)})', flush=True)


def emit_counts(label, values):
    print(f'{label}: {sum(values) / len(values):.1f} ({spread(values)})', flush=True)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--helper', type=Path, required=True,
                        help='the built yapfast-plugins')
    parser.add_argument('--plugin', type=Path, required=True,
                        help='the module the helper holds, once per plugin id')
    parser.add_argument('--pages', type=int, default=256,
                        help='pages of guest memory each plugin is given')
    parser.add_argument('--plugins', type=int, default=4,
                        help='how many plugins the helper holds at once')
    parser.add_argument('--runs', type=int, default=3, help='runs of each measurement')
    parser.add_argument('--settle', type=float, default=2.0,
                        help='seconds to let a process settle before sampling it')
    parser.add_argument('--timeout', type=float, default=120.0,
                        help='seconds to wait for a process to reach its state')
    parser.add_argument('--parent', type=Path,
                        help='the built yapfast, to measure with plugins off and on')
    parser.add_argument('--plugin-dir', type=Path,
                        help='a plugin folder (manifest.json and its module) to install '
                             'for the parent measurement')
    parser.add_argument('--parent-arg', action='append', default=[], metavar='ARG',
                        help='passed to the parent, such as a renderer')
    parser.add_argument('--keep-logs', action='store_true',
                        help='keep the temporary directory, which holds the logs')
    args = parser.parse_args()

    if args.parent and not args.plugin_dir:
        parser.error('--parent needs --plugin-dir, or there is no plugin to measure')

    work = Path(tempfile.mkdtemp(prefix='yapfast-plugins-measure-'))
    log_path = work / 'logs'
    log_path.mkdir()
    try:
        helper_runs = [helper_run(args, run, work / f'cache-{run}', log_path)
                       for run in range(1, args.runs + 1)]
        stages = ['at rest', 'one plugin', 'many plugins']
        for stage in stages:
            emit(f'helper, {stage} RSS',
                 fields(helper_runs, lambda run, stage=stage: run[stage]['rss']))
            emit_counts(f'helper, {stage} threads',
                        fields(helper_runs, lambda run, stage=stage: run[stage]['threads']))
        per_instance = [
            (run['many plugins']['rss'] - run['one plugin']['rss']) / (args.plugins - 1)
            for run in helper_runs]
        emit('per instance, marginal RSS', per_instance)
        gone = sum(1 for run in helper_runs if run['exited']['gone'])
        print(f'helper, after the last eviction: gone in {gone} of {args.runs} runs',
              flush=True)

        if args.parent:
            data = work / 'data'
            data.mkdir()
            installed = subprocess.run(
                [str(args.helper), 'fetch', args.plugin_dir.resolve().as_uri(),
                 '--store', str(data / 'plugins')],
                capture_output=True, text=True)
            if installed.returncode != 0:
                raise Failed(f'`yapfast-plugins fetch` failed: {installed.stderr.strip()}')
            off = [parent_run(args, run, data, True, log_path)
                   for run in range(1, args.runs + 1)]
            on = [parent_run(args, run, data, False, log_path)
                  for run in range(1, args.runs + 1)]
            emit('parent, plugins off RSS', fields(off, lambda s: s['rss']))
            emit_counts('parent, plugins off threads', fields(off, lambda s: s['threads']))
            emit('parent, plugins on RSS', fields(on, lambda s: s['rss']))
            emit_counts('parent, plugins on threads', fields(on, lambda s: s['threads']))
            emit('parent, plugins on helper RSS',
                 fields(on, lambda s: s['helpers'][0]['rss']))
            emit('parent, plugins on total RSS',
                 fields(on, lambda s: s['rss'] + s['helpers'][0]['rss']))
            emit('parent, helpers on threads', fields(on, lambda s: s['helpers'][0]['threads']))
    except Failed as failure:
        print(f'measurement failed: {failure}', file=sys.stderr)
        print(f'logs are in {log_path}', file=sys.stderr)
        return 1
    finally:
        if args.keep_logs:
            print(f'logs and caches are in {work}', flush=True)
        else:
            shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
