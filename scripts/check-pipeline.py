#!/usr/bin/env python3
"""Read-only conformance checks against a disposable run-server.py --explorer --patch fixture."""
import argparse
import json
from pathlib import Path
import statistics
import time
import urllib.error
import urllib.parse
import urllib.request

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--server', default='http://127.0.0.1:8184')
p.add_argument('--fixture', type=Path, help='Save the actual captured ordinary response for Rust tests')
args = p.parse_args()

def render(target='/0x11/app/ios_demo/note', slot='/sys/slot/content', **extra):
    fields = dict(target=target, slot=slot, width='480', textScale='1', direction='ltr', **extra)
    start = time.monotonic()
    with urllib.request.urlopen(urllib.request.Request(args.server+'/goo/render', data=urllib.parse.urlencode(fields).encode()), timeout=120) as r:
        raw = r.read()
    return json.loads(raw), time.monotonic()-start, raw

def walk(v):
    if isinstance(v, dict):
        yield v
        for child in v.values():
            yield from walk(child)
    elif isinstance(v, list):
        for child in v:
            yield from walk(child)

def check(v):
    trace = v['pipeline']
    assert trace['version'] == 1 and trace['origin'] == 'live-preparation'
    stages = {s['name']: s for s in trace['stages']}
    assert list(stages) == ['parse','read','source','core','normalized','visible','semantic','metrics','layout','trace','prepared','display','bindings','html']
    assert stages['parse']['status'] == stages['read']['status'] == 'skipped'
    assert all(s['status'] == 'ok' for name,s in stages.items() if name not in ('parse','read'))
    display = v.get('display') or v['readOnlyDisplay']
    assert stages['display']['output'] == display
    assert stages['bindings']['output'] == v['controls']
    ids = {node['id'] for node in display['nodes']}
    assert ids <= {n.get('attrs',{}).get('id') for n in walk(stages['normalized']['output'])}
    assert ids <= {n.get('attrs',{}).get('id') for n in walk(stages['semantic']['output'])}
    assert ids <= {n.get('id') for n in walk(stages['layout']['output'])}
    assert ids <= set(stages['metrics']['output']['sizes'])
    return len(ids)

plain, _, _ = render()
assert 'pipeline' not in plain
captured, _, raw = render(pipeline='1')
assert check(captured) > 0
assert {k:v for k,v in captured.items() if k != 'pipeline'} == plain, 'Tracing changed the render or action evidence'
assert captured['pipeline']['stages'][-1]['output'] == captured['html']
if args.fixture:
    args.fixture.write_bytes(raw)
synthetic, _, _ = render(slot='/sys/slot/synthetic', pipeline='1')
check(synthetic)
patch, _, _ = render(target='/0x11/app/patch_demo/board', slot='', view='/0x11/gov/patch_demo/editor', pipeline='1')
bodies = [n for n in walk(patch.get('description', [])) if n.get('kind') == 'body' and n.get('display')]
assert bodies, 'Fixture must contain native patch bodies'
for body in bodies:
    check(body)
try:
    render(pipeline='999')
    raise AssertionError('Unsupported capture version accepted')
except urllib.error.HTTPError as error:
    assert error.code == 400
normal = [render()[1] for _ in range(5)]
traced = [render(pipeline='1')[1] for _ in range(5)]
print(json.dumps(dict(passed=True, stages=14, nested_patch_bodies=len(bodies), response_bytes=len(raw),
    warm_normal_median_ms=round(statistics.median(normal)*1000,1),
    warm_capture_median_ms=round(statistics.median(traced)*1000,1)), indent=2))
