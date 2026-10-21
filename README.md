# Goo for Rust and egui

A bounded native desktop consumer of Reaver's **prepared** Goo output. This implements the namespace Explorer workspace, diagram topology, portable patch editing, ordinary prepared editors, and a server-declared deck of independent list/follower panes. The supported milestone-2 surfaces are working; full browser parity remains bounded as listed below.

## Namespace Explorer

```sh
python3 scripts/run-server.py --explorer --port 8164
# In another terminal:
./scripts/cargo-macos.sh run -- --explorer
```

Explorer mode defaults to `GOO_SERVER=http://127.0.0.1:8164`, `GOO_ENDPOINT=/goo-inspector`, and `GOO_PATH=/0x11/app/ios_demo`. Point these at an updated Reaver server and an existing namespace path. The configured endpoint allocates a validated workspace session through `format=json`; the client consumes its installed declaration and host descriptor. No HTML is parsed.

`./scripts/bundle-explorer-macos.sh` builds `target/Shrine Explorer.app` for Finder. The original Goo demo app and CLI modes remain available.

The native shell uses a light slate/teal theme, a compact navigation bar, a narrow namespace sidebar, and a larger Values pane. Each pane owns one scroll viewport; navigation and pane titles stay fixed, with no outer workspace scroller. Patch scenes have a camera-aligned dot grid, framed nodes, and highlighted drag handles. Recipe names are shortened in the picker; hover to see the full current path. Host styling lives in `src/chrome.rs`; prepared Goo boxes, fonts, text and paint still come from Reaver. Scene geometry, including overlapping or oversized authored frames, is preserved.

The unmodified Inspector declaration supplies recursive panes, a namespace outline, a shared selection, a following Values pane, address/Parent navigation, and mount reload/status. Expand branches with the disclosure arrows, select a record, then use the Values pane's recipe picker to open a matching ordinary editor. **Save note** uses Goo actions and rereads persisted content. The source projection refreshes after accepted saves. The outline defaults to compact namespace names, a full-row selection highlight, and full paths on hover. **Details** switches to the supplied prepared child-page `sm`/`md`/`lg` row presentations; these are painted without rewriting their text or layout. Right-click **Inspect row Goo** retains access to each received row packet. Structural branches remain non-selectable. Pages are bounded to 64 children and the materialized tree to 4096 nodes. **Load more children** follows opaque, epoch-bound continuations. Collapsing preserves loaded rows and their owners.

Selection, recipe, path and source changes are staged before retiring editors. Dirty departures offer **Stay** or **Discard draft and continue**; unresolved saves and composition block retirement. Compatible refresh and resize retain drafts and widget identities. Window suspension disables actions; resuming refreshes before re-enabling them and retains drafts. Mount publication requires clean, resolved editors; failures retain the installed workspace. An uncertain rescan is not automatically retried: **Check mount status** observes publication without issuing another scan.

**Received workspace Goo** shows the full workspace response. Ordinary surfaces and native content retain right-click inspection; prepared tree rows can also be inspected, and their context menu exposes the member/bundle contract. Nested row/native packets are extracted from received JSON; their formatted fragment is not claimed to be an independent HTTP response. Values uses the server's explicit read-only prepared display; raw typed-value editing through the browser's custom HTML editor is outside this milestone. Use a declared ordinary editor for changes.

Explorer admits the Inspector's recursive pane/split/stack, tree selection, follower-with-native-fallback, navigation, and configured mount-rescan contracts. Arbitrary Goo decks still use the separate bounded deck mode. The milestone-2 additions below extend the same owner and action machinery.

For disposable shared-fixture conformance, including the 80-child tree and deferred grandchild:

```sh
python3 scripts/verify-explorer.py --reaver /Users/test/dev/reaver --port 8170
```

The runner owns both server and delay proxy, refuses occupied ports, runs scenarios sequentially, and shuts them down afterward. It tests workspace/Values admission, prepared tree paging, stale epochs, out-of-source parents, and the existing real save/unknown-outcome/exact-retry suite. Logs are in `evidence/explorer-*.log`.

## Compilation pipeline inspector

Enable **Capture pipeline**, refresh the workspace/view, then right-click a prepared component (or use **Inspect Goo**). The inspector opens in **Visual pipeline**. Capture is off by default and applies to subsequent requests. An already-open inspection stays frozen; inspect again to capture a newer mounted generation. Local drafts are neither sent nor substituted into compiler output.

**Visual pipeline** paints the complete frozen view through the production painter, with non-editable received input values. Choose a stage to overlay bounds, measured widths, layout rows, adjacency, text-line boxes/baselines, or presented control routes. Click a region for its stage details in the sidebar; select its parent to inspect container rows. Bounds and ID labels can be toggled. Drag to pan, pinch/Ctrl-scroll to zoom, or use **Fit view**. One scene unit is one logical CSS pixel; zoom transforms paint and annotations together without preparing a new layout. Nested fragments have a chooser and retain independent coordinate spaces. Unsupported fragments show validation diagnostics.

The final painted output is the reference for every stage. Correspondence uses explicit node IDs only. Pre-ID source/core, parse/read, and HTML have no geometric mapping; nodes without final boxes remain available in **Pipeline text**. Input controls have no server-supplied text lines, so baseline overlays apply only where lines were supplied. The preview cannot edit or invoke actions.

**Prepared view** shows the complete prepared response, including display nodes, bindings and context, with pipeline/HTML payloads omitted for readability. The view path and target appear at the top of the resizable left sidebar. Modes, stage selection, copy/capture controls, and capture details stay in that sidebar; the right pane is reserved for selectable output, with independent scrolling and text wrapping.

The stage browser shows evaluated **source → core → normalized → visible → semantic → metrics → layout → trace → prepared → display → bindings → HTML**. Stages default to the whole view. When a response includes multiple captured fragments, each is shown with its response path rather than merged into an invented compiler tree. **Render node details** and **Inspect one render node** provide optional drill-down by explicit node ID. Before normalization there are no node IDs, so source/core always show the whole compilation. Copy exports the displayed output, while **Full response** preserves all received fields. Nested native patch bodies and prepared tree rows carry their own captures.

**Pipeline text** defaults to **Readable** formatting: indented declaration trees, measured-size columns, row allocations, relationship decisions, wrapped text lines, display summaries and control routes. Repeated node prefixes use a printed `#n` legend when unambiguous. Numbers are rounded to three decimal places; common bookkeeping and detailed paint/measurement bundles are summarized. **Raw JSON** retains every stage field and exact numeric values; HTML remains literal source text. **Copy output** copies the selected format. These are diagnostic projections of the frozen response, never inputs to rendering or action dispatch.

This requires the accompanying Reaver changes in `extras/mounts/explorer`. Ordinary requests omit pipeline data. Opt-in render requests use `pipeline=1`; stages are attached to the same snapshot as the prepared output, never fetched by a separate replay. Old servers explicitly show “No pipeline captured.” Parse/read artifacts from publication are not retained by live preparation and appear as skipped; synthetic views bypass those stages. Structural pane/tree/patch host contracts remain in the full response, not fabricated Goo stages. Failed render attempts still use the existing error path; the inspector does not retain a partial failed compilation. HTML is selectable text, never executed.

For a separate disposable native demo:

```sh
python3 scripts/run-server.py --explorer --patch --port 8184
# In another terminal:
./scripts/bundle-pipeline-macos.sh
open 'target/Shrine Goo Pipeline.app'
# Read-only cross-stage conformance and warm timing checks:
python3 scripts/check-pipeline.py --server http://127.0.0.1:8184
```

## Milestone 2: diagrams, patches, paint and recipe transitions

```sh
python3 scripts/run-server.py --explorer --patch --tree-fixture --port 8172
# In another terminal:
./scripts/cargo-macos.sh run -- --surfaces
```

`--surfaces` defaults to port 8172 and `/0x11/app/patch_demo`. Select **board**, then choose `/0x11/gov/patch_demo/editor` in Values. `./scripts/bundle-surfaces-macos.sh` builds `target/Shrine Goo Surfaces.app` with those defaults. Existing Explorer environment overrides still apply.

- **Diagram:** the Outline/Diagram switch shares namespace selection, loaded pages and editor owners. Diagram nodes retain server-prepared content sizes; host topology arrangement supplies the links and positions. Zoom, Fit, scrolling and disclosure are available. This arrangement is not pixel-identical to Mash's d3-flextree.
- **Patch:** consumes `contentVersion:1`, complete scene projections, ordered prepared bodies, ports, labels, occurrence lifetimes and the four offered operations. Drag handles move selected nodes; Shift-click selects a group, middle-drag pans, and zoom/Fit control the camera. Grid/alignment snapping and edge panning use the declared policy. Choose two ports to connect. Accessible scene controls expose named moves, reconnect and disconnect. Ordinary operation forms retain drafts. Embedded ordinary references have separate owners and validate their own preparations.
- **Action recovery:** gestures freeze scene/target/topology evidence and use the existing Goo binding. A move preview remains until complete-projection coverage confirms its invocation. Unknown outcomes retain the original body and ID for status lookup or exact retry; hidden completion stays with the originating owner. Node lifetime replacement cannot inherit a previous body's draft or authority. Dirty departure and unresolved work are guarded.
- **Recipes:** ordinary Explorer `become` prepares and stages a guarded recipe change. `invoke-and-become` saves first, then stages the frozen destination only after acceptance. Later typing remains dirty and can refuse departure. Patch-body recipe transitions and recipe-changing views in the original standalone/deck demo are explicitly rejected.
- **Prepared paint:** images/media, avatar/presence, progress gradients, status markers, elliptical/rounded fills and inherited content clips, including clipped native input paint. Bounded asynchronous PNG/JPEG/GIF/WebP/SVG decoding supports data and HTTP(S) sources. Image failures are diagnostic. Text context controls request new server preparations for 1×/1.5×/2× and LTR/RTL; they do not scale or reflow a cached declaration locally.

**Received patch Goo** exposes the exact root response and nested prepared node/control joins. Individual prepared bodies retain component inspection. Host graph lines/handles are interaction chrome; their source geometry and affordances are in the root patch response.

```sh
python3 scripts/verify-surfaces.py --reaver /Users/test/dev/reaver --port 8174
```

This runs six live scenarios against disposable servers and real delayed replies. Ports PORT through PORT+2 must be free. The shared `panes` recipe fixture uses a separate namespace because its structural card role overlaps `ios-demo`; combining their content recipes makes slot selection ambiguous. For a manual recipe fixture use `scripts/run-server.py --no-demo --pane-fixture --explorer --port 8174`, then `GOO_SERVER=http://127.0.0.1:8174 GOO_PATH=/0x11/app/panes/cards ./scripts/cargo-macos.sh run -- --explorer`.

## Patch interaction hardening

Native macOS drags now capture the handle at pointer press and consume the final movement even when press/move/release arrive in one frame. Escape or suspension cancels the capture; an incoming projection cannot silently replace an active gesture. The existing frozen invocation, coverage and exact-retry path handles submission.

Embedded ordinary views use the containing node/label origin plus the supplied reference rectangle, retain their editor/widget identity during movement, and scroll natively within their supplied frame. Unresolved embedded saves expose **Check embedded save status** and **Retry exact embedded save** below the scene. **Gesture diagnostics** provides an opt-in, bounded trace of pointer positions and capture/release state; it records no text input or action body.

To exercise the embedded-owner fixture:

```sh
python3 scripts/run-server.py --explorer --embedded-patch --port 8178
GOO_SERVER=http://127.0.0.1:8178 ./scripts/cargo-macos.sh run -- --surfaces
# Optional Finder app targeting that fixture:
./scripts/bundle-hardening-macos.sh
```

Select **board**, then `/0x11/gov/patch_demo/editor`. The fixture uses Reaver's typed population path for `patch_presentation`; that constructor is not accepted directly by the current static Grove compiler. Its extra scene replacement recipe exists only in the disposable fixture copy, and does not modify the Reaver checkout.

For automated owner-transition verification with an owned server and delay proxy:

```sh
python3 scripts/verify-hardening.py --reaver /Users/test/dev/reaver --port 8180
```

The scenario covers a dirty embedded refresh, a real accepted save with its response delayed past the client deadline, later typing, hidden completion, refused dirty node removal/replacement, resume requiring a fresh preparation, persisted reread, exact retry, and a fresh owner after explicit discard and incarnation replacement. Cold fixture startup can take several minutes; the runner waits for its own server's readiness announcement. See [VERIFICATION.md](VERIFICATION.md) for the measured results and remaining platform gaps.

## Run on macOS

Install Rust, Xcode command-line tools, and the Reaver checkout's Nix dependencies. From this directory, start a disposable server:

```sh
python3 scripts/run-server.py --reaver /Users/test/dev/reaver --port 8142
```

In another terminal:

```sh
./scripts/cargo-macos.sh run
# Ordinary standalone view instead of the declared deck:
./scripts/cargo-macos.sh run -- --single
```

The compiler wrapper and Cargo target configuration use Apple's compiler/linker, avoiding stale Nix shell overrides. Dependencies are pinned in Cargo.lock. The default server checkout is the sibling `reaver`, with the portable contract port on top of main. Read-only preparations allow up to 120 seconds for cold P6 compilation; save and status requests keep their 20-second deadline. The default endpoint is `http://127.0.0.1:8142`. Optional environment variables:

| Variable | Default |
|---|---|
| `GOO_SERVER` | `http://127.0.0.1:8142` |
| `GOO_TARGET` | `/0x11/app/ios_demo/workspace` (deck), `/0x11/app/ios_demo/note` (`--single`) |
| `GOO_WORKSPACE` | `/0x11/gov/ios_demo/editors` |
| `GOO_SLOT` | `/sys/slot/content` (`--single`) |

For a Finder-launchable local app, run `./scripts/bundle-macos.sh` and open `target/Shrine Goo.app`. The script ad-hoc signs the local build. It is not a distributed/notarized application.

Choose a member in either tab, edit TITLE or NOTE, then use the server-declared **Save note** button. The static preview changes only when the server reread arrives. **Refresh view** and window resize preserve compatible dirty inputs. **Reverse tabs** retains owners; **Close tab** and selection changes offer Stay/Discard when needed. Closing a tab is local only. **Refresh declared tabs** restores server declarations, creating new incarnations for previously removed panes. Unknown saves expose **Check save status** and **Retry exact save**; resolve them before removing their owner.

## Inspect received Goo

Right-click any rendered Goo component to open **Received Goo**, or choose **Inspect Goo** in the toolbar to browse the mounted view's nodes (including groups). **Node + bindings** shows the received node's geometry, text lines, fonts, paint, semantics, and controls joined by `binding.nodeId`. **Full response** includes every received field, including fields this renderer does not consume. It is formatted by default; **Verbatim** shows the original body, and **Copy original response** copies that body exactly.

Inspection is a frozen, read-only capture labelled with the originating editor, mounted generation, target, declaration, and publication. Refreshing, switching tabs, or replacing an editor cannot silently retarget it. Right-click again or use **Inspect Goo** to capture the current mounted response. Local unsaved drafts remain separate from server input seeds. Inspection is local and does not issue server requests or activate controls.

The node inspector captures mounted prepared surfaces. Explorer additionally exposes workspace and tree contracts. Rejected responses and unmounted staged replacements are outside this inspector; it is not a response-history recorder.

## Supported slice

- Version-1 `mash-goo` prepared views with pinned fonts, supplied absolute CSS-pixel boxes, explicit lines/baselines, resolved normal/state paint, and semantics.
- Group, text, code, button, single-line input, image/media/avatar/presence and progress bars. Solid/gradient fills, inside borders, rounded/ellipse backgrounds and content clipping, status markers, LTR/RTL prepared placement and text scales 0.25–4. Unsupported descriptors fail before mounting.
- egui text editing, selection, clipboard, focus, IME event integration, scrolling, and AccessKit semantic labels/roles. Stable editor/slot widget IDs retain the caret/undo state through compatible preparation. One CSS pixel equals one egui logical point; screen density affects rasterization. Keyboard UI zoom is disabled.
- A declared deck of up to eight keyed panes, each with one finite direct-child selection/list/follower, at most 64 members. Selections, drafts, generations, and invocations belong to owners independent of widgets.
- Validation/staging before structural retirement, guarded selection/source/member/publication replacement, work-revision and intent checks against superseded guards, creation-only selection seeds, and distinct recreated owners.
- Frozen receiver/subject/publication/control evidence, input values, and selection operands. Accepted/rejected/unknown outcomes remain distinct; retry sends the original bytes and invocation ID. Hidden saves update their original editor. Resume requires a successful fresh preparation before action enablement.

## Deliberate gaps

This uses egui controls hosted by a native macOS window, **not AppKit NSTextField controls**. Native pointer/keyboard editing and AccessKit exposure were exercised; hardware IME composition and VoiceOver workflows were not certified. See the precise evidence in [VERIFICATION.md](VERIFICATION.md).

Static text and action submission remain limited to the pinned ASCII preparation contract; RTL here means supplied placement and input alignment, not Unicode bidi/shaping. Hardware IME, VoiceOver and touch gestures are not certified. Diagram geometry is a native host arrangement, not browser pixel parity.

Patch support is bounded to one scene through pane/split/stack wrappers and the four implemented offers. Query/create/reveal offers, arbitrary nested occurrence arrangements and recipe transitions inside patch bodies are diagnosed before mounting. Connection picking currently uses route bounds, rather than exact curve distance; connection routes remain the supplied projection during a move preview. General Goo deck composition inside Explorer, arbitrary pane alternatives, editable list rows, raw Values editing and other host workflows remain gaps. Embedded references, advanced labels, grid and edge-pan branches have narrower verification than the live patch-demo; see VERIFICATION.md.

No authentication adapter, offline queue, or process-termination recovery exists. Drafts and unresolved invocations survive tab suspension within the process only. Window close is refused while work is dirty/unresolved; forced process termination loses in-memory recovery. There is no detached operation store. Long requests use bounded-time worker threads and stale-ticket rejection rather than transport cancellation. Only macOS was built and inspected.

## Verify

```sh
./scripts/cargo-macos.sh test
./scripts/cargo-macos.sh clippy --all-targets -- -D warnings
python3 scripts/verify-live.py --reaver /Users/test/dev/reaver --port 8148
```

The last command creates an isolated Reaver namespace and a local proxy, runs the opt-in Rust integration test, and terminates both process groups. It never targets an existing namespace. The proxy forwards actual requests and holds actual replies; it does not fabricate Goo output. Ports `PORT` and `PORT+1` must be free. Direct execution of the ignored test requires explicit `GOO_DISPOSABLE=1`, `GOO_TEST_SERVER`, and `GOO_TEST_PROXY` settings and mutates its demo objects.

For a manual hidden-save rehearsal, run `python3 scripts/delay-goo.py --upstream http://127.0.0.1:8142 --port 8151 --seconds 0 --action-seconds 5`, then launch with `GOO_SERVER=http://127.0.0.1:8151 ./scripts/cargo-macos.sh run`. Save Note A and switch tabs before the reply. The proxy delays Note A only; its `/_delay/events.json` endpoint records delivery timing.

[CONTRACTS.md](CONTRACTS.md) records the implementation boundary and provenance. [VERIFICATION.md](VERIFICATION.md) separates deterministic, live-server, and native-window evidence. The shared profile assets retain their upstream font licenses in `assets/`.

## Measure rendering

`python3 scripts/benchmark-render.py --reaver /Users/test/dev/reaver --baseline f373423 --baseline fd7ef90 --output /tmp/goo-render-benchmark`

This archives the requested revisions without changing the checkout, boots each
with the disposable note fixture, and records startup, first render, warm reads,
and width changes separately. It disables the shared PLAN compiler/memo cache
by default; that deliberately tests a much harsher cold start than restarting a
server on a development machine. Use `--warm-cache` to measure with the normal
cache settings. Run on an otherwise idle machine. Results and exact received
responses are saved under the output directory; the script sends no actions and
stops its own server processes afterward.

Text travels through Goo as English runs with a real Foil `myth` for metadata.
Reaver resolves `/sys/font`, `/sys/modifiers` (bold, italic, underline), and
`/sys/link` (internal path or `weblink`) into font identities, line geometry,
underline positions, and filled link boxes. Egui paints the prepared recipes.
Styled displays use version 2; ordinary version-1 responses remain supported.
Link targets are visible on hover and retained as descriptive accessibility
semantics; clicking a highlight does not navigate or invoke an action.

Supported fonts are the pinned sans weights (including matching italics), plus
regular/bold mono. Arbitrary namespace fonts, mono italic, Unicode shaping and
styled native editing are not supported. Ordinary native inputs retain their
existing owners, drafts and save behavior.

Disposable styled-note fixture and read-only verification:

```sh
python3 scripts/run-server.py --reaver ../reaver --port 8198 --explorer --text-fixture
GOO_SERVER=http://127.0.0.1:8198 ./scripts/cargo-macos.sh run -- --single
python3 scripts/check-run-styles.py --server http://127.0.0.1:8198
./scripts/cargo-macos.sh test --test run_styles
```

The original plain-run check remains available as `scripts/check-text-runs.py`
against a disposable server started without `--text-fixture`.

Custom namespace fields can carry `goo/text/pail` and use the explicit
`text(field, slot)` constructor, as in `scripts/text-fixture.grove` and
`scripts/run-server.py`. Automatic `show()` dispatch currently recognizes only
registered types. The core text value remains `row[goo/runt]`.
