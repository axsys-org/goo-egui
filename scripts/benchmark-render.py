#!/usr/bin/env python3
"""Measure disposable Goo HTTP renders, separating startup, first use and warm reads.

By default the machine-wide PLAN compiler/memo cache is disabled. No actions are
sent and no user's server or namespace is used. --baseline REF archives a Git
revision without changing the checkout. Run comparisons on an otherwise idle
machine; startup includes mount publication and is not counted as render time.
"""
import argparse
import io
import json
import os
from pathlib import Path
import signal
import socket
import statistics
import subprocess
import tarfile
import tempfile
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def measure(reaver, label, args):
    # Refuse occupied ports instead of accidentally measuring another server.
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', args.port))
    destination = args.output / label
    destination.mkdir(parents=True, exist_ok=True)
    log_path = destination / 'server.log'
    environment = os.environ.copy()
    if not args.warm_cache:
        environment['PL_CODECACHE'] = '0'
    command = ['python3', str(ROOT / 'scripts/run-server.py'), '--reaver', str(reaver), '--port', str(args.port)]
    with log_path.open('w') as log:
        start = time.monotonic()
        process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            ready = f'Eden: http://127.0.0.1:{args.port};'
            while ready not in log_path.read_text():
                if process.poll() is not None:
                    raise RuntimeError(f'{label} exited during startup: {log_path}')
                if time.monotonic() - start > args.startup_timeout:
                    raise TimeoutError(f'{label} startup timed out: {log_path}')
                time.sleep(.25)
            startup = time.monotonic() - start
            samples = []
            for index, width in enumerate([640] * (args.samples + 1) + [420, 800, 420]):
                body = urllib.parse.urlencode(dict(target='/0x11/app/ios_demo/note',
                    slot='/sys/slot/content', width=width, textScale=1, direction='ltr')).encode()
                start = time.monotonic()
                with urllib.request.urlopen(urllib.request.Request(
                        f'http://127.0.0.1:{args.port}/goo/render', data=body), timeout=args.request_timeout) as response:
                    received = response.read()
                elapsed = time.monotonic() - start
                value = json.loads(received)
                if not value.get('ok') or not value.get('display', {}).get('nodes'):
                    raise RuntimeError(f'{label} did not produce a prepared view')
                (destination / f'response-{index}.json').write_bytes(received)
                samples.append(dict(width=width, seconds=elapsed, bytes=len(received)))
                print(f'{label}: request {index}, width {width}: {elapsed:.3f}s', flush=True)
            result = dict(label=label, compiler_cache='inherited' if args.warm_cache else 'disabled',
                          startup_seconds=startup, first_seconds=samples[0]['seconds'],
                          warm_median_seconds=statistics.median(s['seconds'] for s in samples[1:1+args.samples]),
                          samples=samples)
            (destination / 'timings.json').write_text(json.dumps(result, indent=2) + '\n')
            return result
        finally:
            # The runner creates no other process groups. Terminate its children
            # too, even if the wrapper exited early during a failed startup.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reaver', type=Path, default=ROOT.parent / 'reaver')
    parser.add_argument('--baseline', action='append', default=[], metavar='GIT_REF')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, default=8164)
    parser.add_argument('--samples', type=int, default=4)
    parser.add_argument('--startup-timeout', type=int, default=1200)
    parser.add_argument('--request-timeout', type=int, default=300)
    parser.add_argument('--warm-cache', action='store_true', help='allow the inherited runtime cache settings')
    args = parser.parse_args()
    if args.samples < 1:
        parser.error('--samples must be positive')
    args.reaver = args.reaver.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix='goo-render-benchmark-') as temporary:
        for index, ref in enumerate(args.baseline):
            revision = subprocess.check_output(['git', 'rev-parse', '--verify', ref + '^{commit}'],
                                              cwd=args.reaver, text=True).strip()
            baseline = Path(temporary) / str(index)
            baseline.mkdir()
            archive = subprocess.check_output(['git', 'archive', revision], cwd=args.reaver)
            with tarfile.open(fileobj=io.BytesIO(archive)) as contents:
                contents.extractall(baseline, filter='data')
            # The staging cache validates its source/runtime hash itself. Share
            # it to avoid repeating unrelated boot-compiler preparation.
            if (args.reaver / '.check').is_dir():
                (baseline / '.check').symlink_to(args.reaver / '.check', target_is_directory=True)
            results.append(measure(baseline, revision[:12], args))
        results.append(measure(args.reaver, 'working-tree', args))
    (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
