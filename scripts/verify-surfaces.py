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
parser.add_argument('--port', type=int, default=8174)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
evidence = root / 'evidence'
evidence.mkdir(exist_ok=True)
for port in (args.port, args.port + 1, args.port + 2):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', port))
processes = []
logs = []
try:
    for name, command in [
        ('surfaces-server', ['python3', 'scripts/run-server.py', '--reaver', args.reaver,
                             '--explorer', '--tree-fixture', '--patch', '--port', str(args.port)]),
        ('surfaces-proxy', ['python3', 'scripts/delay-goo.py', '--upstream', f'http://127.0.0.1:{args.port}',
                            '--port', str(args.port + 1), '--seconds', '1', '--action-seconds', '1.5']),
        ('surfaces-patch-proxy', ['python3', 'scripts/delay-goo.py', '--upstream', f'http://127.0.0.1:{args.port}', '--port', str(args.port + 2), '--target', '/0x11/app/patch_demo/board', '--seconds', '0', '--action-seconds', '1.5']),
    ]:
        log = open(evidence / f'{name}.log', 'w')
        logs.append(log)
        processes.append(subprocess.Popen(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, start_new_session=True))
    deadline = time.monotonic() + 900
    while f'Eden: http://127.0.0.1:{args.port};' not in (evidence / 'surfaces-server.log').read_text():
        if any(p.poll() is not None for p in processes):
            raise RuntimeError('Fixture process exited; inspect evidence/surfaces-*.log')
        if time.monotonic() >= deadline:
            raise TimeoutError('Disposable Explorer startup exceeded 900 seconds')
        time.sleep(1)
    env = dict(os.environ, GOO_DISPOSABLE='1', GOO_SERVER=f'http://127.0.0.1:{args.port}',
               GOO_TEST_SERVER=f'http://127.0.0.1:{args.port}', GOO_TEST_PROXY=f'http://127.0.0.1:{args.port + 1}')
    # Workspace allocation changes the namespace epoch: scenarios must be sequential.
    for suite in ('explorer', 'live', 'patch'):
        env['GOO_TEST_PROXY'] = f'http://127.0.0.1:{args.port + (2 if suite == "patch" else 1)}'
        with open(evidence / f'surfaces-{suite}-live.log', 'w') as output:
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

# The panes fixture and ios-demo intentionally use the same structural card role.
# A separate namespace avoids making /sys/slot/content ambiguous for both.
recipe_log = evidence / 'surfaces-recipes-server.log'
with open(recipe_log, 'w') as output:
    server = subprocess.Popen(['python3', 'scripts/run-server.py', '--reaver', args.reaver,
        '--no-demo', '--pane-fixture', '--explorer', '--port', str(args.port)], cwd=root,
        stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        deadline = time.monotonic() + 900
        while f'Eden: http://127.0.0.1:{args.port};' not in recipe_log.read_text():
            if server.poll() is not None: raise RuntimeError('Recipe server exited')
            if time.monotonic() >= deadline: raise TimeoutError('Recipe server startup')
            time.sleep(1)
        with open(evidence / 'surfaces-recipes-live.log', 'w') as log:
            started = time.monotonic()
            subprocess.run(['./scripts/cargo-macos.sh', 'test', '--test', 'recipes', '--',
                '--ignored', '--nocapture'], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            print(f'recipes: passed ({time.monotonic() - started:.2f}s)', flush=True)
    finally:
        if server.poll() is None: os.killpg(server.pid, signal.SIGTERM)
        try: server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(server.pid, signal.SIGKILL)
            server.wait()
