#!/usr/bin/env python3
"""Benchmark JSON vs protobuf on one isolated, immutable Reaver namespace.

No actions are sent. Startup/cold preparation are excluded. Both modes are
warmed, case order is shuffled each round, and each case gets a balanced shuffled
JSON-first/protobuf-first schedule. Captures are checked for stability and decoded
by the release Rust harness for semantic parity and allocation measurements.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import signal
import socket
import statistics
import subprocess
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def cases():
    view = dict(target='/0x11/app/ios_demo/note', slot='/sys/slot/content',
                width='480', textScale='1', direction='ltr')
    source = dict(target='/0x11/app/ios_demo/notes', axis='z')
    return [
        ('rich-480', '/goo/render', True, view),
        ('rich-960', '/goo/render', True, dict(view, width='960')),
        ('deck', '/goo/render', True, dict(target='/0x11/app/ios_demo/workspace',
            view='/0x11/gov/ios_demo/editors', width='700', textScale='1', direction='ltr')),
        ('source', '/goo/source', False, source),
        ('tree', '/goo/source', False, dict(source, topology='tree', paged='1')),
        ('status', '/goo/status', False, dict(target=view['target'])),
        ('receipt', '/goo/operation-status', False,
            dict(target=view['target'], invocation='no-such-invocation')),
    ]


def stats(values):
    ordered = sorted(values)
    return dict(medianMs=statistics.median(values),
                p95Ms=ordered[math.ceil(len(ordered)*.95)-1], minMs=min(values), maxMs=max(values))


def fetch(base, case, binary):
    name, path, post, original = case
    fields = dict(original)
    if binary:
        fields['wire'] = 'protobuf-v2'
    encoded = urllib.parse.urlencode(fields).encode()
    request = urllib.request.Request(base + path + ('' if post else '?' + encoded.decode()),
        data=encoded if post else None, headers={'Content-Type': 'application/x-www-form-urlencoded',
        'Accept-Encoding': 'identity'})
    start = time.perf_counter_ns()
    with urllib.request.urlopen(request, timeout=180) as response:
        headers_at = time.perf_counter_ns()
        payload = response.read()
        end = time.perf_counter_ns()
        mime = response.headers.get_content_type()
        expected = 'application/x-protobuf' if binary else 'application/json'
        if response.status != 200 or mime != expected:
            raise RuntimeError(f'{name}: {response.status}, {mime}, expected {expected}')
        if response.headers.get('Content-Encoding', 'identity') != 'identity':
            raise RuntimeError('Unexpected compression')
    return payload, dict(totalMs=(end-start)/1e6, headersMs=(headers_at-start)/1e6,
                         bodyMs=(end-headers_at)/1e6)


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    capture = output / 'captures'
    capture.mkdir(exist_ok=True)
    log_path = output / 'server.log'
    # Never attach a mutating fixture or a benchmark to another server accidentally.
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', args.port))
    environment = os.environ.copy()
    environment['PL_CODECACHE'] = '1'
    command = ['python3', str(ROOT / 'scripts/run-server.py'), '--reaver', str(args.reaver),
               '--rich-input', '--port', str(args.port)]
    with log_path.open('w') as log:
        start = time.monotonic()
        process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            ready = f'Eden: http://127.0.0.1:{args.port};'
            while ready not in log_path.read_text():
                if process.poll() is not None:
                    raise RuntimeError(f'Server exited: {log_path}')
                if time.monotonic() - start > 1200:
                    raise TimeoutError(f'Server startup timeout: {log_path}')
                time.sleep(.25)
            startup = time.monotonic() - start
            print(f'Server ready in {startup:.1f}s; warming both formats', flush=True)
            all_cases = cases()
            base = f'http://127.0.0.1:{args.port}'
            captures, samples, first_use = {}, {}, {}
            for case in all_cases:
                name = case[0]
                samples[name] = []
                for warm in range(args.warmup):
                    for binary in ([False, True] if warm % 2 == 0 else [True, False]):
                        payload, timing = fetch(base, case, binary)
                        if warm == 0:
                            first_use[name + ('/protobuf' if binary else '/json')] = timing
                        captures[name, binary] = payload
                (capture / (name + '.json')).write_bytes(captures[name, False])
                (capture / (name + '.pb')).write_bytes(captures[name, True])
                print(f'Warmed {name}', flush=True)
            rng = random.Random(args.seed)
            schedules = {}
            for case in all_cases:
                order = [False, True] * (args.samples // 2)
                rng.shuffle(order)
                schedules[case[0]] = order
            for round_number in range(args.samples):
                order = all_cases.copy()
                rng.shuffle(order)
                for case in order:
                    name = case[0]
                    binary_first = schedules[name][round_number]
                    pair = dict(round=round_number, protobufFirst=binary_first)
                    for binary in [binary_first, not binary_first]:
                        payload, timing = fetch(base, case, binary)
                        if payload != captures[name, binary]:
                            raise RuntimeError(f'{name} changed during read-only benchmark')
                        pair['protobuf' if binary else 'json'] = timing
                    samples[name].append(pair)
                if (round_number + 1) % 10 == 0:
                    print(f'Completed {round_number + 1}/{args.samples} rounds', flush=True)
            report = []
            for case in all_cases:
                name = case[0]
                rows = samples[name]
                diffs = [p['protobuf']['totalMs']-p['json']['totalMs'] for p in rows]
                boot = sorted(statistics.median(rng.choices(diffs, k=len(diffs))) for _ in range(4000))
                item = dict(case=name, fields=case[3], jsonBytes=len(captures[name, False]),
                    protobufBytes=len(captures[name, True]),
                    json=stats([p['json']['totalMs'] for p in rows]),
                    protobuf=stats([p['protobuf']['totalMs'] for p in rows]),
                    pairedMedianDeltaMs=statistics.median(diffs),
                    pairedMedianDelta95BootstrapMs=[boot[100], boot[3899]], samples=rows)
                report.append(item)
            metadata = dict(timestamp=datetime.now(timezone.utc).isoformat(), machine=platform.platform(),
                python=platform.python_version(), samplesPerFormatPerCase=args.samples, warmupPerFormat=args.warmup,
                randomSeed=args.seed, concurrency=1, compression='identity', startupSeconds=startup,
                wireVersion=2, decoderTarget='typed domain models; no inspector projection', compilerCache='PL_CODECACHE=1; cold first uses excluded', firstUse=first_use,
                reaverHead=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.reaver, text=True).strip(),
                serverWorkingDiffSha256=hashlib.sha256(subprocess.check_output(['git', 'diff'], cwd=args.reaver)).hexdigest(),
                sourceSha256={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [
                    args.reaver / 'src/foil/wire_proto.foil', args.reaver / 'src/reaver/bat.rvr',
                    args.reaver / 'extras/mounts/goo/protocol/goo.proto',
                    args.reaver / 'extras/mounts/goo/wire.foil',
                    args.reaver / 'extras/mounts/goo/wire_prepared.foil',
                    args.reaver / 'extras/mounts/explorer/goo_wire_browser.foil',
                    ROOT / 'src/protobuf.rs', ROOT / 'src/wire_schema.rs', ROOT / 'src/model.rs', ROOT / 'Cargo.lock']})
            (capture / 'cases.json').write_text(json.dumps([c[0] for c in all_cases]))
            (output / 'http.json').write_text(json.dumps(dict(metadata=metadata, results=report), indent=2)+'\n')
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    # No server or concurrent workload during client decoding timings.
    binary = ROOT / 'target/release/goo-transport-bench'
    with (output / 'decode.json').open('w') as decoded:
        subprocess.run([str(binary), str(capture)], cwd=ROOT, stdout=decoded, check=True)
    print(f'Results: {output}', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reaver', type=Path, default=ROOT.parent / 'reaver')
    parser.add_argument('--port', type=int, default=8210)
    parser.add_argument('--samples', type=int, default=40)
    parser.add_argument('--warmup', type=int, default=5)
    parser.add_argument('--seed', type=int, default=20261022)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/transport-benchmark')
    args = parser.parse_args()
    if args.samples < 20 or args.samples % 2 or args.warmup < 2:
        parser.error('Use an even sample count >=20 and at least 2 warmup reads per format')
    args.reaver = args.reaver.resolve()
    run(args)


if __name__ == '__main__':
    main()
