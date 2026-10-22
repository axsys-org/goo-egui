#!/usr/bin/env python3
"""Boot disposable Reaver + delay proxy, run Rust live test, terminate both process groups."""
import argparse
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reaver', default=str(Path(__file__).resolve().parents[2] / 'reaver'))
parser.add_argument('--port', type=int, default=8144)
parser.add_argument('--protobuf', action='store_true', help='Also compare JSON and protobuf responses and warm request timings')
parser.add_argument('--rich-input', action='store_true', help='Run the rich-text input fixture and integration test')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
(root / 'evidence').mkdir(exist_ok=True)
# Refuse occupied ports: never run a mutating test against somebody else's server.
for port in (args.port, args.port + 1):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', port))
processes = []
logs = []
try:
    commands = [
        ('integration-server', ['python3', 'scripts/run-server.py', '--reaver', args.reaver, '--port', str(args.port), *(['--rich-input'] if args.rich_input else [])]),
        ('integration-proxy', ['python3', 'scripts/delay-goo.py', '--upstream', f'http://127.0.0.1:{args.port}',
                               '--port', str(args.port + 1), '--seconds', '1', '--action-seconds', '1.5']),
    ]
    for name, command in commands:
        log = open(root / 'evidence' / f'{name}.log', 'w')
        logs.append(log)
        processes.append(subprocess.Popen(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, start_new_session=True))
    # Only this newly launched server's own ready announcement permits mutation.
    ready = f'Eden: http://127.0.0.1:{args.port};'
    for _ in range(600):
        if any(process.poll() is not None for process in processes):
            raise RuntimeError('Fixture process exited; inspect evidence/integration-*.log')
        if ready in (root / 'evidence/integration-server.log').read_text():
            break
        time.sleep(1)
    else:
        raise RuntimeError('Disposable server startup timeout')
    environment = os.environ.copy()
    environment.update(GOO_DISPOSABLE='1', GOO_TEST_SERVER=f'http://127.0.0.1:{args.port}',
                       GOO_TEST_PROXY=f'http://127.0.0.1:{args.port + 1}')
    subprocess.run(['./scripts/cargo-macos.sh', 'test', '--test', 'rich_input' if args.rich_input else 'live', '--', '--ignored', '--nocapture'],
                   cwd=root, env=environment, check=True)
    if args.protobuf:
        subprocess.run(['./scripts/cargo-macos.sh', 'test', '--test', 'protobuf', '--', '--ignored', '--nocapture'], cwd=root, env=environment, check=True)
finally:
    for process in processes:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
    for process in processes:
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
    for log in logs:
        log.close()
