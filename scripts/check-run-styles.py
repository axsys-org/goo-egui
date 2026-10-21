#!/usr/bin/env python3
"""Read-only checks against run-server.py --text-fixture."""
import argparse
import json
from pathlib import Path
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--server', default='http://127.0.0.1:8196')
parser.add_argument('--fixture', type=Path)
args = parser.parse_args()
results = []
for width, scale, direction in [(180, 1, 'ltr'), (480, 1, 'ltr'), (360, 2, 'rtl')]:
    data = urllib.parse.urlencode(dict(target='/0x11/app/ios_demo/note', slot='/sys/slot/content',
        width=width, textScale=scale, direction=direction, pipeline=1)).encode()
    with urllib.request.urlopen(urllib.request.Request(args.server+'/goo/render',data=data),timeout=180) as response:
        p = json.load(response)
    assert p['ok'], p
    styled = [n for n in p['display']['nodes'] if any(r['myth'] for r in n['textRuns'])]
    assert len(styled) == 1
    n = styled[0]
    fragments = [r for l in n['lines'] for r in l['runs']]
    fonts = {r['resolved']['textStyle']['fontId'] for r in fragments if 'resolved' in r}
    assert {'sans750','italic400','italic750','mono700'} <= fonts
    links = [r for r in fragments if r.get('resolved',{}).get('semantics',{}).get('role') == 'link']
    assert {r['resolved']['semantics']['target']['kind'] for r in links} == {'path','weblink'}
    assert all(r['resolved']['highlight']['fill'] and r['resolved']['highlight']['width'] > 0 for r in links)
    assert all(c['binding']['nodeId'] != n['id'] for c in p['controls'])
    assert len(p['controls']) == 3
    for line in n['lines']:
        assert ''.join(r['text'] for r in line['runs']) == line['text']
        assert abs(sum(r['box']['width'] for r in line['runs'])-(line['box']['width']-line['prefixAdvance'])) < 1e-6
    if width == 180:
        assert sum(r['resolved']['semantics']['target']['kind'] == 'path' for r in links) >= 2
    if width == 480 and args.fixture:
        args.fixture.write_text(json.dumps(p,indent=2)+'\n')
    results.append(dict(width=width, scale=scale, direction=direction, lines=len(n['lines']), link_fragments=len(links), controls=len(p['controls'])))
print(json.dumps(dict(passed=True,cases=results),indent=2))
