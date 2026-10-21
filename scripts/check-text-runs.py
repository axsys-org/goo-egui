#!/usr/bin/env python3
"""Read-only text-run checks against a disposable run-server.py server."""
import argparse
import copy
import json
import math
from pathlib import Path
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--server', default='http://127.0.0.1:8188')
parser.add_argument('--fixture', type=Path)
args = parser.parse_args()


def render(width):
    fields = dict(target='/0x11/app/ios_demo/note', slot='/sys/slot/content',
                  width=str(width), textScale='1', direction='ltr', pipeline='1')
    request = urllib.request.Request(args.server + '/goo/render',
                                    data=urllib.parse.urlencode(fields).encode())
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def plain(runs):
    assert all(run['myth'] == {} and set(run['lang']) == {'en'} for run in runs)
    return ''.join(run['lang']['en']['txt'] for run in runs)


def check(p):
    assert p['ok']
    stages = {s['name']: s for s in p['pipeline']['stages']}
    assert stages['display']['output'] == p['display']
    assert stages['bindings']['output'] == p['controls']
    for node in p['display']['nodes']:
        assert plain(node['textRuns']) == node['payload']
        for line in node['lines']:
            assert ''.join(run['text'] for run in line['runs']) == line['text']
            x = line['textX']
            for run in line['runs']:
                assert run['lang'] == 'en' and run['myth'] == {}
                assert math.isclose(run['textX'], x, abs_tol=1e-6)
                assert run['baseline'] == line['baseline']
                x += run['box']['width']
            assert math.isclose(x - line['textX'], line['box']['width'] - line['prefixAdvance'], abs_tol=1e-6)
    for record in stages['prepared']['output']['lines']:
        if record['lines'] is not None:
            assert [plain(runs) for runs in record['runLines']] == record['lines']
    return len(p['display']['nodes'])


wide, narrow = render(480), render(160)
counts = [check(p) for p in [wide, narrow]]
legacy = json.loads((Path(__file__).resolve().parents[1] / 'tests/fixtures/live-pipeline.json').read_text())
projection = copy.deepcopy(wide['display'])
for node in projection['nodes']:
    node.pop('textRuns')
    for line in node['lines']:
        line.pop('runs')
assert projection == legacy['display'], 'Plain-view paint/geometry changed'
assert wide['controls'] == legacy['controls'], 'Control authority changed'
if args.fixture:
    args.fixture.write_text(json.dumps(wide, indent=2) + '\n')
print(json.dumps(dict(passed=True, widths=[480, 160], nodes=counts,
                      legacy_display_identical=True, bindings_identical=True), indent=2))
