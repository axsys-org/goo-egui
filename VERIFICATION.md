# Verification — 2026-10-19

Target: Apple Silicon macOS, Rust 1.98.1, eframe/egui 0.33.3, native OpenGL window, AccessKit enabled. Reaver: P6 compiler, disposable `ios-demo` mount from `scripts/demo`, portable checkout and source hashes in `evidence/provenance.json`.

## Deterministic checks

`./scripts/cargo-macos.sh test`: **24 passed, 0 failed**; the mutating live test is intentionally ignored by default. `./scripts/cargo-macos.sh clippy --all-targets -- -D warnings`: **passed**. Logs: `evidence/tests.log`, `evidence/clippy.log`.

Tests cover profile/context/font rejection, malformed geometry and duplicate nodes, hidden metadata, descriptive buttons without authority, stale response generations, dirty refresh/resize, changed-lifetime staging, guard refusal and supersession by new edits/intents, composition deferral, exact immutable invocation/capture bytes, edits during saves, accepted/rejected/unknown distinctions, independent drafts, keyed reorder, fresh incarnation after recreation, hidden-owner completion, stale actions on resume, unresolved-save retirement refusal, invalid-candidate atomicity, and source replacement.

Painter tests render the real note fixture through egui, drive pointer/keyboard events into its TextEdit, and check cursor retention across refresh. Pinned font advance totals (including tracking) match server lines within 0.25 CSS px; egui glyph advance versus pinned em metrics is checked within 0.05 CSS px. These checks caught and corrected total-inset/per-edge confusion and redundant font-size scaling.

The 14 shared server display fixtures are enumerated. `row-wrap` and `empty` are admitted in the reduced ordinary contract; the other 12 explicitly reject unsupported primitives, markers, context, or absent input bindings. This is **not a claim of full 14-fixture renderer conformance**. The ordinary interactive note fixture separately passes full binding admission and actual painting.

## Real server integration

`python3 scripts/verify-live.py --reaver /Users/test/dev/reaver-two --port 8154`: **1 live test passed, 0 failed**, 3.84 seconds for the test after server startup. The runner booted a fresh namespace and terminated its server/proxy process groups afterward. `evidence/integration-hardened.log` and `evidence/integration-proxy.log` retain output. This final run used occupied-port refusal and the newly launched server's own readiness announcement before mutation.

The Rust live test performs:

1. An actual `/goo/render` admission, edit, `/goo/act` save and persisted reread; response saved as `evidence/live-reread.json`.
2. A dirty editor preparation at a different width without losing its working value.
3. An actual server-declared two-tab deck, real source membership and separate selected subjects/drafts.
4. An actual Note A mutation through a proxy that holds its real reply for 1.5 seconds, exceeding a 350 ms client timeout. The hidden origin retains the unknown invocation; dirty B remains unchanged; removal of A is refused.
5. An accepted receipt from Reaver and a byte-identical invocation retry. The object revision before/after retry is identical, demonstrating no second mutation.
6. Completion applied to the hidden origin, resume action gating until reread, an old delayed render ignored after a newer ticket, reordered draft retention, Stay/superseded removal rejection, and explicit discard/removal.

This is a Rust transport/session integration test using the same production client/session code as the UI. It does not automate a whole OS keyboard/IME workflow.

## Native macOS inspection

The actual packaged `.app` was launched and inspected with screenshots and its AccessKit accessibility tree. Verified:

- Supplied prepared text, fonts, input boxes, and Save button render without a browser.
- Pointer/keyboard title editing works; Refresh preserves the draft and focused field.
- Save changes the static preview title/body on server reread and clears the dirty indicator.
- Separate First/Second selections and dirty drafts survive switching and local reordering.
- Dirty tab removal presents Stay/Discard. Stay preserves both owners; explicit Discard removes only the selected tab and leaves the other draft intact.
- Window resize preserves the dirty first-tab input while re-preparing geometry.

Screenshots: `evidence/native-dirty-resize.png`, `evidence/native-removal-guard.png`. The native inspection server was booted before the demo subtitle was updated, so those screenshots contain the original fixture string “Rendered with UIKit.” The client is Rust/egui; no UIKit or HTML renderer is involved. Fresh demo launches say “Rendered with Rust and egui.”

A separate native delayed-save rehearsal was started but not completed; its result is not counted. Hidden delayed/unknown completion is verified by the real-server Rust integration above. Native hardware IME composition, VoiceOver navigation/announcements, OS quit/relaunch recovery, other operating systems, and browser pixel-equivalence are not verified. AccessKit exposes fields/buttons and supplied labels; accessibility SetValue was not effective in the manual tool, while pointer/keyboard editing worked. Do not interpret semantic-tree presence as full assistive-technology certification.

## Scope and residual limits

This paragraph describes the original slice; the namespace Explorer follow-up below supersedes its tree and host-operation boundaries. No HTML parsing, guessed role palette, or second Goo layout engine was introduced. Pending recovery remains in memory. General prepared-summary lists, richer paint primitives, and recipe-changing bindings are deferred explicitly.

## Received-Goo inspector follow-up

After adding inspection, `./scripts/cargo-macos.sh test` passes **27 tests, 0 failures** (24 existing plus 3 inspector tests). The mutating live test remains ignored and was not rerun for this change. Strict Clippy and the macOS bundle build pass. Logs: `evidence/inspection-tests.log`, `evidence/inspection-clippy.log`.

The new tests verify byte-for-byte response retention, preservation of unknown envelope/node fields, node-to-binding joins, frozen owner and mounted-generation attribution while refresh is pending and after replacement, separation of local drafts from received inputs, and actual egui secondary-pointer clicks on buttons/inputs without actions or edits.

The rebuilt native macOS app was inspected against the running disposable server at port 8142. Right-clicking Save opened its `invoke` binding without activating it; right-clicking TITLE opened its input node and `/sys/req` binding. A frozen generation-2 inspection remained at generation 2 through Refresh; explicit Inspect Goo recaptured generation 3. The full response included fields omitted by the typed renderer (including HTML, role/config, projection sequence, and covered operation IDs). The final formatted full-response view and Verbatim control were visually checked. Screenshot: `evidence/native-inspector.png`. Clipboard contents were not independently read back in the OS check; exact original-body retention is covered deterministically.

## Main-based server port follow-up

Reaver's portable contracts now run on main `f373423` in `/Users/test/dev/reaver` (port commit `fd7ef90`).
`evidence/main-port-tests.log` records 27 passing deterministic tests;
`evidence/main-port-clippy.log` records strict Clippy success.
`evidence/integration-main-final.log` records the full disposable integration test
passing in 15.42 seconds against the ported server: save/reread, dirty resize,
declared independent tabs, hidden completion, unknown timeout, receipt recovery,
byte-identical retry without a second mutation, stale response and guarded removal.

Two earlier runs timed out on their first render with the former 20-second
deadline. Read-only preparation now has a separate 120-second budget; the
save/status deadline is still 20 seconds, and explicit `with_timeout` overrides
retain their requested deadline for all endpoints. Observed diagnostic render
times were 38.06 seconds cold and 0.848 seconds warm on the ported server, versus
12.549/0.132 seconds on an unmodified-main disposable baseline. These measurements
are not controlled benchmarks; performance overhead remains a known limitation.

The separately launched **Shrine Goo Main** macOS app connected to the disposable
ported server at port 8158. Native pointer/keyboard editing set the title to
“Saved through ported main”; Save completed and the static preview displayed
the persisted reread with no dirty state. Right-click inspection showed its
received invoke binding. Screenshot: `evidence/native-main-reread.png`.
The normal app bundle was rebuilt; server runners and README now default to
the sibling `reaver` checkout.


## Renderer performance follow-up — 2026-10-19

Reaver's generated paint code now uses one shared JSON builder. All eight full
HTTP responses at 640, 420, and 800 CSS px are byte-identical to the original
port. The optimization does not change any native client state or action logic.

With shared compiler/memo caches disabled, first requests were 81.124 s on main,
88.036 s on the original port, and 82.185 s after the optimization. Corresponding
warm medians were 140, 154, and 155 ms. Servers finished startup before serial
measurements began; all verification jobs had finished. With the normal cache,
a first-use optimized-code probe took 7.751 s. Cache populations differ between
normal-cache runs, so that is not a matched 38-to-8-second speedup measurement.
A fully cold P6 runtime remains slow even on main.

Validation: 9 profile integrity tests; 36 compiled recipe parity cases; 15 native
prepared assertions; 14 Chromium/SVG fixtures (79 boxes, 48 lines); 777 native
web/Explorer/moodboard assertions and 2 mount contracts. The real-server Rust
integration passed in 14.91 s, including persisted reread, draft retention,
hidden completion, unknown outcomes, receipts, exact retry, stale responses,
and guarded removal.

See [raw timings and provenance](evidence/render-performance.json),
[response hashes](evidence/performance-response-parity.json),
[profile log](evidence/performance-profile.log),
[native contract log](evidence/performance-native.log), and
[live integration log](evidence/performance-live.log).

The new benchmark script also passed an end-to-end run with `--warm-cache`:
63.101 s of server startup, followed by a 0.198-second first HTTP render and a
0.155-second warm median. See [script results](evidence/performance-benchmark-script.json).


## Namespace Explorer milestone — 2026-10-19

The packaged **Shrine Explorer.app** consumes the installed Inspector workspace,
including namespace tree pages, prepared row bundles, follower/Values content,
ordinary recipe selection, path/Parent navigation, and mount publication status.
The server additions preserve the browser HTML flow and expose JSON workspace
entry plus a separate read-only prepared Values display. They are committed in
Reaver as `c9017b8`, on the existing main-based `lf/goo-contracts` branch.
The Rust client remains in the non-Git `/Users/test/dev/shrine-egui` workspace.

Final automated results:

- **39 deterministic Rust tests passed**, 0 failures; 3 live tests intentionally
  ignored in the default run. New cases cover atomic page admission, epoch/life
  joins, duplicate/missing preparations, scope identity, compatible draft merges,
  frozen pending saves, replacement, refusal, and structural-response supersession.
  [Test log](evidence/explorer-tests.log).
- Strict Clippy (`--all-targets -- -D warnings`), formatting, Python script
  compilation, and the ad-hoc signed macOS bundle build passed.
  [Clippy log](evidence/explorer-clippy.log).
- **86 Explorer Node tests passed**, 0 failures.
  [Node log](evidence/explorer-node-tests.log).
- **9 profile tests and 745 compiled native assertions passed**: web 135,
  mount contracts 2, Explorer 608. Suite times were 80.2, 109.8, and 149.8 seconds
  respectively; these are verification runtimes, not rendering benchmarks.
  [Native summary](evidence/explorer-native-contracts.log),
  [Explorer assertions](evidence/explorer-native-explorer.log).
- `python3 scripts/verify-explorer.py --reaver /Users/test/dev/reaver --port 8170`
  passed against its own fresh disposable namespace and proxy, then stopped them.
  **2 Explorer live scenarios passed in 16.07 s**: installed session/host/workspace,
  prepared Values, 80-child paging (64 + 16), deferred grandchild, stale epoch and
  out-of-source rejection. **1 existing save/recovery scenario passed in 5.54 s**:
  actual save/reread, dirty resize, declared independent deck, hidden completion,
  timeout/unknown, receipt recovery, byte-identical retry without second mutation,
  delayed stale render, and guarded removal. These are test times excluding
  compilation and server startup.
  [Explorer live log](evidence/explorer-explorer-live.log),
  [Save/recovery log](evidence/explorer-live-live.log).

Native macOS checks used a separate disposable server at port 8164. Screenshots
and AccessKit state were inspected during interaction. Verified the Inspector
layout, tree expansion and prepared rows, synthetic Values, ordinary editor
selection, physical pointer/keyboard editing, Save and persisted preview reread,
whole-workspace refresh retaining draft/caret, dirty selection refusal with Stay,
window resize retaining the draft, frozen received-Goo inspection, successful
mount reload/status, and Parent navigation. A final save to “Explorer source
refresh verified” refreshed source evidence automatically; expanding notes then
returned a/b/c without a stale-epoch error. Minimizing disabled Save; reopening
from Finder refreshed before re-enabling Save and retained “Draft retained across
suspension” and its focused field. That disposable draft was discarded afterward.
The final app is left showing the persisted saved value and expanded namespace.

The delayed/unknown rehearsal is a production transport/session integration test,
not a native-window timing test. Hardware IME and VoiceOver remain uncertified.
Mount-publication failure recovery is covered by existing server/browser tests
and client state handling, but only successful publication was exercised in the
native UI. No full-browser pixel parity or performance benchmark was run for this
milestone. The read-only Values display does not enable the HTML raw slot editor.
Patch/diagram gestures, general decks in Explorer mode, unsupported paint/text
features, authentication, and recovery after process termination remain gaps.

## Surface expansion / milestone 2 — 2026-10-19

Client-only changes against main-based Reaver `c9017b8`; no server code changes.
The earlier milestones above are historical, including their unsupported-feature
lists. README.md contains the current capability boundary.

- **54 deterministic Rust tests passed**, 0 failures; **6 live tests ignored** in
  the default run. [Final log](evidence/surfaces-tests.log).
  Added patch admission/offer joins, malformed identity/geometry/ports, frozen
  gestures, occurrence replacement, dirty refusal, stale projection/encounter,
  retired embedded-owner responses, stale action IDs, explicit operation coverage,
  native egui multi-frame drag accumulation/release, nested Goo inspection, declared
  snap tolerance, diagram geometry, gradient hard stops, clipping, bounded SVG
  decoding, and frozen recipe destinations.
- **11 of 14 shared display fixtures admit and tessellate** through the actual
  egui painter at their supplied context. The other three contain editable inputs
  but no finalized bindings in this display-only corpus and correctly remain
  unavailable. Real bound inputs are exercised separately. This is geometry and
  rendering-path verification, not a pixel-diff against UIKit or HTML.
- Strict Clippy (`--all-targets -- -D warnings`), formatting, Python compilation,
  and the signed macOS bundle build passed.
  [Clippy](evidence/surfaces-clippy.log), [build](evidence/surfaces-build.log).
- `scripts/verify-surfaces.py --port 8174` passed **all six live scenarios**, with
  its own disposable servers/proxies and automatic cleanup. The shared recipe
  fixture runs in a separate namespace because its role overlaps ios-demo and
  makes their `/sys/slot/content` selection ambiguous if both are installed.
  Test execution times below exclude compilation and server startup:
  - Explorer workspace/host/Values and paged-tree scenarios: **2 passed, 10.98 s**.
    [Log](evidence/surfaces-explorer-live.log).
  - Original save/deck/recovery scenario: **1 passed, 5.61 s**, including persisted
    reread, dirty resize, hidden completion, delayed stale response, unknown outcome,
    status recovery and byte-identical retry without a second mutation.
    [Log](evidence/surfaces-live-live.log).
  - Patch scenarios: **2 passed, 5.62 s**, including persisted move/connect/reconnect/
    disconnect, explicit coverage, exact retry, real delayed reply timeout, hidden
    originating owner and receipt recovery. [Log](evidence/surfaces-patch-live.log).
  - Recipe scenario: **1 passed, 1.01 s**, including actual become/edit binding,
    invoke-and-become save, scale-2 RTL preparation, persisted reread, and a later
    dirty draft refusing departure. [Log](evidence/surfaces-recipes-live.log).

Native macOS inspection used a separate disposable server at port 8172 and the
signed Shrine Goo Surfaces.app. Inspected screenshots and AccessKit state for the
three prepared patch nodes, ports, connection line/arrow, diagram topology and
named scene controls. Executed native accessible moves and connect/reconnect in
the earlier working build. In the final UI verification, physically clicked and
typed `draft retained` into the ordinary operation input, staged a different
selection, chose Stay, switched Outline to Diagram and refreshed the patch: the
same draft remained. Discarded this disposable draft, then used Move source right;
the actual server reread changed Source x=80 to x=90 and retained y=80.
[Received persisted response](evidence/surfaces-native-reread.json).

Verification limits: the automated native drag attempts did not produce a persisted
move, so physical-window dragging is **not certified** by this run. The deterministic
real-egui pointer-event test passes multi-frame drag and immutable release capture;
named native move controls passed end to end. Touch, hardware IME and VoiceOver
remain uncertified. Embedded references, advanced labels, grid and edge-pan behavior
have admission/unit coverage or implementation inspection but no full live-fixture
UI rehearsal. Expanded paint was exercised through egui tessellation and SVG decode,
not a native screenshot matrix. Recipe transitions and hidden/unknown patch outcomes
were live transport/session tests, not native-window timing tests. No performance
benchmark or browser pixel-parity claim is made for this milestone.

Remaining functional boundaries: one portable patch scene through supported
wrappers; four offered mutation kinds; no query/create/reveal offers or arbitrary
nested occurrence layouts; no patch-body recipe transitions; no generic decks in
Explorer; no raw Values editing; ASCII preparation/submission only. Route picking
uses bounds and preview wires retain supplied projection geometry. Diagram topology
uses a native host arrangement rather than Mash's d3-flextree. See CONTRACTS.md for
admission and ownership details. Recovery remains in-process only.

After final request-context validation and server-relative image URL fixes, the full
54-test deterministic suite, strict Clippy and signed bundle were rerun successfully.
The live persisted patch operation scenario was also rerun against the task-owned
8172 fixture: **1 passed, 3.95 s**.
[Final live regression](evidence/surfaces-final-patch-live.log).
The packaged app and disposable demo server are left available; the extra delay
proxy and the integration runner's servers were stopped.

## Patch interaction hardening — 2026-10-19

This section supersedes the milestone-2 note that physical native dragging was not certified.

Changes under test: pointer capture through coalesced native events; embedded reference positioning/scrolling; embedded save recovery controls; stale embedded error rejection; preparation deferral during workspace staging. Reaver remains at `c9017b8` with no tracked server changes from this work. The integration fixture is a temporary copy of patch-demo extended through the implemented typed `goo_patch/population` contract.

Final checks:

- `cargo test`: **61 passed, 0 failed, 7 opt-in live tests ignored** (`evidence/hardening-tests.log`).
- `cargo clippy --all-targets -- -D warnings`: passed (`evidence/hardening-clippy.log`). `cargo fmt -- --check`: passed.
- Python syntax checks for `run-server.py` and `verify-hardening.py`: passed. Both bundle shell scripts pass `sh -n`.
- Native debug Surfaces and Hardening app bundles built and ad-hoc signed. `hardening-build.log` and `hardening-surfaces-build.log` record the builds.
- Two final live patch scenarios: **2 passed**, 5.28 seconds. Persisted move/connect/reconnect/disconnect; explicit invocation coverage; unknown outcome, hidden originating owner, receipt lookup, byte-identical retry with unchanged persisted revision. See `hardening-patch-live.log`.
- Final embedded-owner scenario: **1 passed**, 6.57 seconds. Real Reaver plus a proxy holding the real action reply 1.5 seconds; client deadline 250 ms. Dirty compatible refresh, frozen inputs despite later typing, unknown outcome, actual node-incarnation replacement and removal through the fixture's Goo action, refusal preserving the old owner, hidden completion, resume remaining stale until refresh, persisted reread, exact retry without another mutation, and a new owner after explicit discard. See `hardening-embedded-live.log` and `hardening-proxy.log`.
- The live checks above used manually owned disposable servers/proxies on 8172/8181 and 8178/8179. The new `verify-hardening.py` wrapper was syntax checked; its constituent server, proxy and test commands were executed successfully. The wrapper itself was not separately rerun through another cold server boot.

Deterministic additions cover native movement+release and complete press/move/release coalesced into one frame, nested Explorer scrolling, Escape/suspension cancellation, refusal to replace a captured gesture, embedded input placement following a node and optimistic preview without changing editor identity/draft, stale embedded failures, and deferral of preparations while still delivering save outcomes during workspace staging. Existing guard supersession and incarnation tests remain passing.

Native macOS inspection and interaction used the actual app windows through CUA:

1. The original trace showed a press and final movement/release with no egui `drag_started` signal. After the fix, dragging Source persisted `x=100 → 178.96958923339844`; the native window displayed “Patch refreshed from Reaver” with the moved selection. Exact server packets: `hardening-native-before.json` and `hardening-native-after.json`.
2. The embedded note rendered supplied prepared text, native input fields and Save. Typing `Native embedded save`, refreshing, attempting departure, and choosing Stay retained the draft and usable editor. Save changed the static native preview and a separate `/goo/render` reread confirmed persistence (`hardening-native-note.json`).
3. Typing the later draft `Draft survives moving`, then physically dragging its containing Source node preserved the draft and focused input/caret while the node moved and reread. The server confirmed `x=40 → 118.96958923339844` (`hardening-native-embedded-drag.json`).
4. Initial native resume exposed a preparation/workspace race. The final build defers preparation completions during workspace staging; repeating initial background-to-foreground resume produced “Explorer ready” and enabled controls after fresh preparations, without the spurious supersession message.

Eight warm real HTTP patch-root preparations measured **205.54 ms median**, **230.97 ms maximum** (`hardening-warm-render.json`). This measures the root response, including reference descriptors, and excludes the separate embedded view request, GPU frame time, and cold compilation. No full paint-gallery or GPU performance certification is claimed.

Remaining boundaries: advanced query/create/reveal offers, arbitrary Explorer decks and patch-body recipe transitions are still outside this slice. Touch gestures, broad IME/VoiceOver behavior, overlapping-surface hit arbitration, and full Unicode shaping remain uncertified. The fixture's embedded frame is intentionally larger than the original demo node rectangle; the client preserves those supplied dimensions rather than inventing layout. The new interaction trace is opt-in, retains at most 32 records in memory, and records no editor text or action body.

## Native shell polish — 2026-10-19

Added shared host styling, grouped declaration navigation controls, bounded sidebar proportions, panel frames, a quieter status bar, shorter recipe labels with full-path hover, and patch canvas/handle styling. Declaration identities, editor ownership, prepared content paint, action routes and scene coordinates are unchanged. Selecting a namespace member now uses its path basename instead of the generic “Select” label.

- Full deterministic suite: **61 passed, 0 failed, 7 opt-in live tests ignored** (`evidence/polish-tests.log`).
- Existing contract/input tests rerun with the production theme installed: **24 passed**, including native keyboard input and cursor preservation (`evidence/polish-themed-inputs.log`).
- Strict clippy and formatting passed (`evidence/polish-clippy.log`). Surfaces and Hardening debug app bundles built and signed.
- Inspected the actual macOS windows through CUA before/after: grouped address controls, sidebar/Values proportions, prepared embedded inputs, recipe menu, canvas grid, node cards and selection outline. The embedded stress fixture still overflows its intentionally smaller supplied node rectangle; no client relayout was added.
- Physically dragged Source left in the disposable server on port 8172 using the styled handle. The app showed “Applying offered patch operation…” followed by “Patch refreshed from Reaver”, enabled controls again, and displayed the moved Source separately from Transform. No new live save/unknown-outcome suite was run for this appearance-only pass; those results above are from the preceding hardening pass.

## Compact namespace sidebar — 2026-10-19

The outline now defaults to one namespace basename per row, a full-row selection target/highlight, compact indentation, and painted disclosure arrows. Full paths are hover text. **Details** opts into the original server-prepared row presentation instead of the compact name; **Inspect row Goo** remains available in each row's context menu. Prepared labels/controls are not duplicated or rewritten, and compact mode does not mount their hidden widgets. The selection route still uses the validated tree member, with the existing editor guards.

`cargo test`: **62 passed, 0 failed, 7 opt-in live tests ignored** (`evidence/sidebar-tests.log`). Strict clippy passed (`evidence/sidebar-clippy.log`); formatting and the native Surfaces bundle build passed. A new pointer test verifies that clicking a row label selects it, disclosure and child-control clicks do not also select it, and unavailable rows cannot be selected. Native macOS inspection confirmed single-label rows and disclosure rendering. Selecting a row opened Values; toggling Details swapped the compact name for the received prepared summary; collapsing a branch hid its children without selecting the branch.


## Pane-owned scrolling — 2026-10-19

Removed the workspace scroll wrapper and the nested outline/diagram/follower scroll wrappers. Each framed pane now owns one bounded scroll viewport with its title outside it. Split panes use the remaining workspace height instead of a 520px minimum; narrow stacked panes divide that height instead of extending the page. Embedded editor scrolling within a patch remains part of that component's supplied frame.

- **63 deterministic tests passed**, 0 failed, 7 opt-in live tests ignored (`evidence/scroll-tests.log`). Strict clippy (`evidence/scroll-clippy.log`), formatting and the Surfaces app build passed.
- Added a bounded-pane regression: oversized content scrolls within its pane without growing the workspace, moving its header, or changing the neighboring pane's scroll offset.
- Native macOS check: scrolled the long debugger Values presentation down two pages. Its content and single scrollbar moved; the namespace, navigation, both pane titles, pane borders and status bar stayed in place. Restored the previous debugger selection and expanded debug/ios_demo branches.

## Live compiler pipeline inspector — 2026-10-21

Added opt-in `pipeline=1` capture in Reaver's actual ordinary/synthetic/patch-body preparation path and a **Compilation pipeline** tab in the egui **Received Goo** window. Captures share the mounted response and remain frozen to its owner/generation. The client displays server data, follows explicit IDs, retains unknown fields, and never submits drafts or derives compiler output. Capture is disabled by default; the checkbox affects subsequent render/tree-presentation requests. Existing open captures do not silently update.

- **68 deterministic Rust tests passed, 0 failed, 7 opt-in live tests ignored** (`evidence/pipeline-tests.log`). Five new tests cover pre-ID whole-stage output, explicit ID/metric/trace/binding joins, frozen capture across refresh and owner replacement, absent/unsupported versions, nested-fragment scoping, and an actual captured server fixture. Strict clippy, formatting, and signed Pipeline/Surfaces app builds passed.
- Disposable real Reaver server launched on **8184**, with `ios-demo`, `goo-inspector`, and `patch-demo`. `scripts/check-pipeline.py` passed: 14 stage entries, identical normal/captured ordinary output after removing the optional field, matching prepared display and action bindings, matching normalized/semantic/layout/metric identities, synthetic Values capture, **7 nested patch body captures**, and rejection of unsupported capture version 999. This check is read-only and does not replace the ignored save/unknown-outcome suites.
- Warm ordinary median **182.2 ms**, captured median **346.8 ms**, five requests each on the small editable note fixture. Captured response **92,088 bytes**. The first cold capture took **27.475 seconds** while warming the new server's render/compiler path; these warm medians are not cold-start guarantees or benchmarks for large trees.
- Inspected the real macOS **Shrine Goo Pipeline** window via CUA: enabled capture, selected the note, opened inspection, viewed the evaluated typed source, selected `Request`, and followed its ID through layout (`w=863`, `h=15.95`), final display (baseline/font/paint/accessibility), and semantic adjacency. Parse showed `skipped` and an explicit uncaptured-stage explanation. Left the app showing the selected semantic stage. The original running app/session was not restarted.
- Boundaries: source begins after receiver evaluation; publication-time parse/read artifacts are unavailable and truthfully marked skipped. Structural host contracts remain in raw received output. No partial failed-compilation capture. Before IDs exist, the UI shows the complete source/core; HTML stays text. Capture increases response size and is best enabled only while inspecting. Existing response size limits remain in force.

Run commands are in README's **Compilation pipeline inspector** section. The separate Pipeline app uses port 8184; other app bundles need their Reaver server rebuilt/reloaded with these changes before they can receive stage captures.


## Inspector sidebar layout — 2026-10-21

Moved all inspection controls, stage selection, component navigation, metadata, diagnostics, and copy/capture actions into a resizable left sidebar. The right pane uses the window's full content height for selectable output. Long lines wrap to its width; each side scrolls independently. Component labels are compact, with exact IDs on hover and the selected ID shown below the list. Capture details are collapsible.

Verified the corrected two-pane layout in the actual macOS Pipeline app: the window encloses both panes, output starts at the top of the right pane, and the sidebar can show its lower capture details while output remains at its beginning. Eight existing inspection tests passed; strict clippy, formatting, and Pipeline/Surfaces bundle builds passed. No server or action changes.

## Whole-view inspection — 2026-10-21

The inspector now defaults to the complete prepared view, with the view path and target prominent and render-node navigation hidden until requested. Compilation stages default to whole-view output, independent of the selected node. Multiple captured fragments retain their response paths and separate outputs. Full response still preserves the exact received fields; Prepared view omits pipeline/HTML payloads for readability. This inspects evaluated runtime output, not original authored `@view` source or view-call boundaries.

Nine focused inspection tests passed, including the new whole-view selection-independence, multi-fragment aggregation and unsupported-fragment-version regression (`evidence/whole-view-inspection-tests.log`). Strict clippy, formatting and signed Pipeline/Surfaces builds passed. Native macOS verification against the disposable server on 8184 confirmed the default prepared output contains all seven display nodes and context, without a node list, and Compilation pipeline opens with “Inspect one render node” off and the complete source stage visible. No server or action changes; the full integration/save suite was not rerun for this inspector-only change.

## Readable pipeline printers — 2026-10-21

Added cached stage-specific text projections and a Readable / Raw JSON toggle. Source/core/normalized/visible/semantic use indented trees; metrics use aligned columns; layout retains nested row allocations; trace lists relationship decisions; prepared lists wrapped text; display summarizes geometry, fonts, paint and semantics; bindings show node/slot/route/capture evidence. Multiple fragments keep their paths, statuses and diagnostics. HTML remains literal source. Common bookkeeping and detailed measurement/paint data remain in Raw JSON; readable numbers round to three decimals, and unambiguous node prefixes use an explicit legend. Copy output follows the selected format. No rendering, action or server behavior changed.

- **13 focused tests passed, 0 failed**: nine existing inspection tests and four new formatter tests using the actual captured fixture. Coverage includes every supplied stage, fragment identity/status, unknown stages/shapes, escaped text, contradictory binding joins and preservation of the original data (`evidence/pipeline-pretty-tests.log`).
- Strict clippy across all targets and formatting passed (`evidence/pipeline-pretty-clippy.log`). Pipeline and Surfaces bundles built and signed.
- Inspected the updated native macOS Pipeline app against the disposable server on 8184. The synthetic source now reads as seven indented nodes; metrics show aligned MIN/PREFERRED/GROW columns; layout shows all seven nodes, row ranges, gaps and rounded sizes. Toggled Raw JSON and verified the complete metric map remained available, then returned to Readable layout. The editable captured fixture's source formats into 14 lines in the deterministic test.
- The full integration/save suite was not rerun for this diagnostic-only change. Prepared view and Full response retain their existing JSON presentation.

## Painted pipeline overlays — 2026-10-21

Inspection now opens on **Visual pipeline**, a frozen painted preview with stage annotations. The shared production painter supplies the paint, fonts, images, clipping and non-editable received input values. Snapshot mode takes an immutable presentation and creates no invocation targets. The native editor path retains its existing widget identities and working values. Each fragment is independently validated before previewing; unsupported geometry, profiles or bindings produce diagnostics. Fragment selection scopes both coordinates and stage data.

Overlays use explicit IDs and supplied final boxes/lines: bounds, minimum/preferred width marks, container-row bands, adjacency arrows, final text-line boxes/baselines and presented control routes. Clicking picks the topmost region respecting ancestor clipping; details stay in the sidebar. Drag pans and zoom scales paint and annotations together; Fit view resets the viewport. Source/core, parse/read and HTML remain final-paint references without invented node correspondence. The readable and raw text modes remain available. Detailed geometry absent from a stage is not reconstructed.

- Full deterministic suite: **78 passed, 0 failed, 7 opt-in live tests ignored** (`evidence/inspection-overlay-tests.log`). Five added tests cover explicit geometry joins, hidden/absent bindings, fragment validation and frozen data, rounded ancestor clipping/paint order, non-editable snapshot painting, and drag/zoom without changing preparation width or selecting during a drag.
- Strict clippy across all targets and formatting passed (`evidence/inspection-overlay-clippy.log`). Pipeline and Surfaces bundles built and signed.
- Actual native macOS inspection against disposable Reaver on 8184: opened synthetic Values and the ordinary editable note, verified the whole painted content, clicked a text region to see its exact node, viewed final line boxes/baselines, and highlighted both input routes and Save. Clicking the preview's Save region selected binding `0.4.0.4`; it did not invoke a save. Labels were moved outside regions after native inspection caught text obstruction; baseline markers were changed from arrows to straight lines. Native scroll-panning moved paint and overlays together, and Fit view restored their framing. Drag and zoom passed deterministic egui input tests; the CUA drag gesture did not visibly move the canvas, so native drag behavior remains unconfirmed.
- No server changes or live save/invocation test reruns in this pass. General screen-reader navigation of visual regions and large-scene performance are not certified. Fragments are shown separately, not recomposed into a host scene.
## Language-tagged text runs — 2026-10-21

The initial descriptor representation recorded here is superseded by the actual-myth correction below.

Goo text is now an ordered row of runs containing independent `RunDescriptor` metadata (`myth`) and an English language payload. Descriptors use open JSON object fields; they are unrelated to Shrine myth values or action authority. Compiler passes, codecs, normalization, slicing and wrapping preserve run boundaries and metadata. Preparation supplies fragment positions and baselines; egui validates those fragments and paints at the supplied origins. Plain string constructors lift into one empty-descriptor English run, and legacy version-1 responses remain accepted.

- **207 Foil checks passed, 0 failed**, using the pinned Wisp compiler/runtime against the current source. The fresh snapshot log contained no top-level compiler errors. **15 prepared fixtures** emitted without errors, including a word split across two source runs and wrapped across three lines (`evidence/goo-text.json`, `evidence/goo-text-fixtures.json`). Checks cover descriptor round trips, invalid languages/envelopes, codec projection mismatches, metadata preservation, CRLF across runs, tab expansion, and measurement/wrapping invariance under run partitioning.
- Full Rust suite: **84 passed, 0 failed, 7 opt-in live tests ignored** (`evidence/text-runs-tests.log`). Six text-run tests cover legacy compatibility, language/descriptor rejection, projection and geometry validation, painting the actual Foil fixture, and mounting an actual live response. Strict clippy across all targets passed (`evidence/text-runs-clippy.log`); formatting, native build and Reaver diff whitespace checks passed.
- Read-only conformance against the disposable Reaver server on **8188** passed at **480px and 160px**, with 14 nodes each. After removing the validated additive run fields, the wide display equals the previous live fixture exactly; control bindings are identical (`evidence/text-runs-live.log`). The historical comparison helper also validated and projected the actual display/prepared capture against the old fixture. The complete Haskell differential suites were not run.
- Inspected the signed **Shrine Goo Text Runs** macOS app against that server. Text, labels, native inputs and Save rendered correctly. A dirty title survived Refresh and window resize; saving produced a clean persisted reread showing the changed title. Restored the disposable note to `Hello from Reaver` and saved again. IME/composition and delayed/unknown-save live scenarios were not rerun in this pass; existing deterministic owner/action tests passed as part of the full suite.
- Supported rendering remains **English/ASCII with empty descriptors**. Arbitrary descriptor metadata round trips through the IR, but nonempty descriptors are explicitly rejected during preparation and client validation until their property semantics are defined. This representation is an extensible field map, not a new language-level row-polymorphism feature. Rich-text editing, additional languages, Unicode shaping and UIKit consumption of run fragments remain outside this change.


## Actual myth run metadata — 2026-10-21

Removed `goo/run_descriptor`. `goo/runt` now declares `: =myth lang=goo/lang`, using the existing `mop[path pail lt_pith]` type directly. Empty English runs use `myth/new`; compiler passes, normalization and slicing retain the actual typed pails. JSON conversion is confined to the wire adapter. Its current explicit pail cases are nat, bool, text and path; foreign brands emit an unsupported diagnostic and are rejected on decode instead of being coerced. Nonempty myths still require rendering semantics before preparation can accept them.

- **212 Foil checks passed, 0 failed** (`evidence/goo-myth-text.json`). The compiler/runtime log contained no top-level errors. Added checks exercise typed myth slot round trips, custom pail brand preservation and slicing, unsupported wire brands, and canonical slot paths.
- All **15 prepared fixture outputs are exactly identical** to the preceding implementation, including HTML and fragment geometry (`evidence/goo-myth-text-fixtures.json` versus `evidence/goo-text-fixtures.json`).
- All **6 focused egui text-run tests passed** (`evidence/myth-text-tests.log`); formatting and Reaver diff whitespace checks passed. No renderer behavior changed. The full Rust suite, live server save flow and native UI inspection were not rerun for this source-type correction; their preceding results remain recorded above.


## Myth-driven run styling — 2026-10-21

Runs keep the actual `myth` type. `/sys/font` carries a typed path, `/sys/modifiers` carries a typed list of italic/bold/underline, and `/sys/link` carries a typed internal path or `weblink { href=str }`. Reaver resolves pinned fonts, actual advances, wrapping, shared baselines, underline placement, link fill/stroke and descriptive targets. egui validates those prepared recipes and paints them directly. Styled displays use version 2; plain displays remain version 1. Link metadata creates no control binding or navigation authority.

- **226 Foil checks passed**, with no top-level compiler errors; **18 prepared fixtures** emitted. All **15 preceding plain fixture outputs are exactly unchanged**, including HTML and geometry. Evidence: `evidence/run-styles-foil.json` and `evidence/run-styles-fixtures.json`.
- Deterministic Rust coverage includes actual prepared fixtures at narrow/wide/scaled RTL contexts, actual live authored-view responses, font identity/geometry/style validation, legacy version rejection for styled output, forged/missing recipes, server-supplied fill painting and absence of navigation commands. Full suite: **88 passed, 0 failed, 7 opt-in live tests ignored**. Strict clippy passed. Results are in `evidence/run-styles-tests.log` and `evidence/run-styles-clippy.log`; formatting and native build passed.
- **Live disposable Reaver on 8198 passed**: widths 180/480 at scale 1 LTR and width 360 at scale 2 RTL. Styled content occupied 5/2/5 lines, with 4/3/4 highlighted link fragments and exactly three declared controls. Captured response: `evidence/run-styles-live-view.json`; results: `evidence/run-styles-live.log`. Custom namespace text is carried by `goo/text/pail` and rendered with explicit `text(note.styled, '/sys/slot/body)`; automatic `show()` dispatch does not register arbitrary custom types.
- Inspected **Shrine Goo Run Styles** on native macOS: bold, italic, bold italic, underline, bold mono, and filled bordered internal/web links rendered at supplied baselines. Window narrowing wrapped link boxes across lines. An edited title survived Refresh and resize, then Save yielded a clean persisted reread showing the new title. Restored the disposable title afterward. Read-only link accessibility nodes are present; full screen-reader operation was not certified.
- All six added font files match the manifest SHA-256 in Reaver, explorer assets and egui. They derive from existing pinned font sources, with the generator and provenance in `extras/mounts/goo/run-fonts/`.
- Limits: English/ASCII; pinned sans and mono font registry only; mono italic and styled native inputs explicitly rejected. Arbitrary namespace font loading, Unicode shaping, rich-text editing, and link navigation are not implemented. UIKit is unchanged and should reject styled display v2. HTML emits styling, but browser mixed-font baseline/line-height parity has not been verified. Delayed/unknown-save and IME live scenarios were not rerun in this pass; existing deterministic owner/action tests remain in the full suite.
