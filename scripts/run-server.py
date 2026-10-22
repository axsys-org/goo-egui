#!/usr/bin/env python3
"""Boot an isolated real Reaver namespace with the editable Goo demo."""
import argparse
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reaver', type=Path, default=Path(__file__).resolve().parents[2] / 'reaver')
parser.add_argument('--port', type=int, default=8142)
parser.add_argument('--explorer', action='store_true')
parser.add_argument('--no-demo', action='store_true', help='Omit ios-demo when testing overlapping structural roles')
parser.add_argument('--pane-fixture', action='store_true', help='Add shared recipe/capture fixtures')
parser.add_argument('--patch', action='store_true', help='Add the real portable patch demo')
parser.add_argument('--embedded-patch', action='store_true', help='Disposable patch with an embedded note and scene replacement editor')
parser.add_argument('--tree-fixture', action='store_true', help='Add disposable deep/wide tree conformance records')
parser.add_argument('--text-fixture', action='store_true', help='Include typed styled text in the disposable editable note')
parser.add_argument('--rich-input', action='store_true', help='Enable disposable rich-text editing and a typed save action')
args = parser.parse_args()
if args.rich_input:
    args.text_fixture = True
root = args.reaver.resolve()
if not (root / 'x/eden').is_file():
    parser.error('Pass --reaver /path/to/reaver')
with tempfile.TemporaryDirectory(prefix='goo-egui-') as temporary:
    extras = Path(temporary)
    mounts = extras / 'mounts'
    mounts.mkdir()
    for mount in (root / 'extras/mounts').iterdir():
        if mount.is_dir():
            (mounts / mount.name).symlink_to(mount, target_is_directory=True)
    (mounts / 'ios-demo').symlink_to(Path(__file__).resolve().parent / 'demo', target_is_directory=True)
    if args.text_fixture:
        if args.no_demo:
            parser.error('--text-fixture requires ios-demo')
        demo = mounts / 'ios-demo'
        demo.unlink()
        shutil.copytree(Path(__file__).resolve().parent / 'demo', demo)
        (demo / 'text-fixture.foil').write_text((Path(__file__).resolve().parent / 'text-fixture.foil').read_text())
        manifest = json.loads((demo / 'mount.json').read_text())
        manifest['modules']['text_fixture'] = 'text-fixture.foil'
        manifest['roots'] = [['lib', name] for name in manifest['modules']] + [g['root'] for g in manifest['grove']]
        manifest['grove'][0]['prelude'] = 'text_fixture'
        (demo / 'mount.json').write_text(json.dumps(manifest, indent=2))
        source = (demo / 'demo.grove').read_text()
        source = source.replace('  req: text\n  res: text', '  req: text\n  res: text\n  %/styled: %/run_text_type', 1)
        source = source.replace('    show(note.res,', '    text(note.styled, \'/sys/slot/body)\n    show(note.res,', 1)
        source = source.replace('\n  res: "', '\n  %/styled: text_fixture/value\n  res: "')
        prefix = (Path(__file__).resolve().parent / 'text-fixture.grove').read_text()
        if args.rich_input:
            source = source.replace('action = .poke', 'action = %/save_rich')
            source = source.replace("      '/sys/res = .input(note.res)", "      '/sys/res = .input(note.res)\n      '/0x11/gov/ios_demo/styled = .input(note.styled)")
            source = source.replace('        action("Save note")', '        show("RICH TEXT - select text and right-click to format", \'/sys/slot/label)\n        input(\'/0x11/gov/ios_demo/styled)\n        action("Save note")')
            prefix = prefix.replace('    ?> value=goo/text/pail value .none\n    | ./some value', '    | goo_edit/decode value')
            source += (Path(__file__).resolve().parent / 'rich-input.grove').read_text()
        (demo / 'demo.grove').write_text(prefix + source)
    if args.embedded_patch:
        if args.no_demo:
            parser.error('--embedded-patch requires ios-demo')
        args.patch = True
        patch = mounts / 'patch-demo'
        patch.unlink()
        shutil.copytree(root / 'extras/mounts/patch-demo', patch)
        grove = patch / 'main.grove'
        source = grove.read_text()
        start = source.index('    patch_node("source") {')
        end = source.index('    patch_node("transform") {', start)
        source = source[:start] + source[end:]
        source = source.replace('  %/scene: text', '  %/scene: text\n  %/population: %/population_type')
        source = source.replace('scene = board.scene,', 'scene = board.scene, population = board.population,')
        source = source.rstrip() + '\n  %/population: (fixture_patch/population patch_demo/initial)\n'
        source = (Path(__file__).resolve().parent / 'patch-types.grove').read_text() + source
        source += (Path(__file__).resolve().parent / 'patch-fixture.grove').read_text()
        contract = patch / 'contract.foil'
        contract.write_text(contract.read_text() + (Path(__file__).resolve().parent / 'patch-fixture.foil').read_text())
        grove.write_text(source)
    if args.tree_fixture:
        (mounts / 'tree-pages').symlink_to(root / 'extras/tests/fixtures/tree-pages', target_is_directory=True)
    if args.pane_fixture:
        (mounts / 'panes').symlink_to(root / 'extras/tests/fixtures/panes', target_is_directory=True)
    print(f'Goo demo: http://127.0.0.1:{args.port}', flush=True)
    print('Workspace: /0x11/app/ios_demo/workspace | deck: /0x11/gov/ios_demo/editors (single pane: /0x11/gov/ios_demo/workspace)', flush=True)
    print('Single view: /0x11/app/ios_demo/note | slot: /sys/slot/content', flush=True)
    try:
        subprocess.run([str(root / 'x/eden'), '--extras-root', str(extras), * ([] if args.no_demo else ['--mount', 'ios-demo']),
                        *(['--mount', 'goo-inspector'] if args.explorer else []),
                        *(['--mount', 'patch-demo'] if args.patch else []),
                        *(['--mount', 'panes'] if args.pane_fixture else []),
                        *(['--mount', 'tree-pages'] if args.tree_fixture else []),
                        '--port', str(args.port)], cwd=root, check=True)
    except KeyboardInterrupt:
        pass
