#!/usr/bin/env python3
"""Run the native Explorer contracts and save recovery against disposable shared fixtures."""
import argparse
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reaver', default=str(Path(__file__).resolve().parents[2] / 'reaver'))
parser.add_argument('--port', type=int, default=8170)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
evidence = root / 'evidence'
evidence.mkdir(exist_ok=True)
for port in (args.port, args.port + 1):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', port))
processes = []
logs = []
try:
    for name, command in [
        ('explorer-server', ['python3', 'scripts/run-server.py', '--reaver', args.reaver,
                             '--explorer', '--tree-fixture', '--port', str(args.port)]),
        ('explorer-proxy', ['python3', 'scripts/delay-goo.py', '--upstream', f'http://127.0.0.1:{args.port}',
                            '--port', str(args.port + 1), '--seconds', '1', '--action-seconds', '1.5']),
    ]:
        log = open(evidence / f'{name}.log', 'w')
        logs.append(log)
        processes.append(subprocess.Popen(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, start_new_session=True))
    deadline = time.monotonic() + 900
    while f'Eden: http://127.0.0.1:{args.port};' not in (evidence / 'explorer-server.log').read_text():
        if any(p.poll() is not None for p in processes):
            raise RuntimeError('Fixture process exited; inspect evidence/explorer-*.log')
        if time.monotonic() >= deadline:
            raise TimeoutError('Disposable Explorer startup exceeded 900 seconds')
        time.sleep(1)
    env = dict(os.environ, GOO_DISPOSABLE='1', GOO_SERVER=f'http://127.0.0.1:{args.port}',
               GOO_TEST_SERVER=f'http://127.0.0.1:{args.port}', GOO_TEST_PROXY=f'http://127.0.0.1:{args.port + 1}')
    # Workspace allocation changes the namespace epoch: scenarios must be sequential.
    for suite in ('explorer', 'live'):
        with open(evidence / f'explorer-{suite}-live.log', 'w') as output:
            started = time.monotonic()
            subprocess.run(['./scripts/cargo-macos.sh', 'test', '--test', suite, '--', '--ignored',
                            '--nocapture', '--test-threads=1'], cwd=root, env=env, stdout=output,
                           stderr=subprocess.STDOUT, check=True)
            print(f'{suite}: passed ({time.monotonic() - started:.2f}s)', flush=True)
finally:
    for p in processes:
        if p.poll() is None:
            os.killpg(p.pid, signal.SIGTERM)
    for p in processes:
        try:
            p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()
    for log in logs:
        log.close()
