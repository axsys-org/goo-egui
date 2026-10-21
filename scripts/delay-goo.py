#!/usr/bin/env python3
"""Test-only proxy: delay a real Goo detail response, without fabricating output."""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--upstream', default='http://127.0.0.1:8132')
parser.add_argument('--port', type=int, default=8133)
parser.add_argument('--seconds', type=float, default=8)
parser.add_argument('--action-seconds', type=float, default=0, help='Hold an actual accepted action reply for tab-switch tests')
parser.add_argument('--target', default='/0x11/app/ios_demo/notes/a')
args = parser.parse_args()
events = []
lock = threading.Lock()

def record(event, target, slot):
    item = dict(event=event, target=target, slot=slot, time=time.monotonic())
    with lock:
        events.append(item)
    print(json.dumps(item), flush=True)

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.forward()

    def do_POST(self):
        self.forward()

    def log_message(self, *args):
        pass

    def forward(self):
        if self.path == '/_delay/events.json':
            with lock:
                body = json.dumps(events).encode()
            self.respond(200, 'application/json', body)
            return
        data = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        fields = urllib.parse.parse_qs(data.decode()) if data else {}
        target = fields.get('target', [''])[0]
        slot = fields.get('slot', [''])[0]
        request = urllib.request.Request(args.upstream.rstrip('/') + self.path, data=data if self.command == 'POST' else None,
                                         method=self.command, headers={'Content-Type': self.headers.get('Content-Type', 'application/x-www-form-urlencoded')})
        try:
            response = urllib.request.urlopen(request, timeout=45)
        except urllib.error.HTTPError as error:
            response = error
        except urllib.error.URLError as error:
            self.respond(502, 'application/json', json.dumps({'error': str(error)}).encode())
            return
        with response:
            body = response.read()
            status = response.status
            mime = response.headers.get('Content-Type', 'application/json')
        if self.path == '/goo/render' and target == args.target and slot == '/sys/slot/content':
            record('delaying-real-response', target, slot)
            time.sleep(args.seconds)
        if self.path == '/goo/act' and target == args.target and args.action_seconds > 0:
            record('delaying-real-action-response', target, slot)
            time.sleep(args.action_seconds)
        self.respond(status, mime, body)
        record('response-delivered', target, slot)

    def respond(self, status, mime, body):
        try:
            self.send_response(status)
            self.send_header('Content-Type', mime)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The native consumer can cancel the retired request.

print(f'Delay proxy: http://127.0.0.1:{args.port} -> {args.upstream}', flush=True)
ThreadingHTTPServer(('127.0.0.1', args.port), Handler).serve_forever()
