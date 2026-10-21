# Contract audit

The initial audit and milestone-1 sections below are historical. The current milestone-2 boundary is recorded at the end; the server remains main-based Reaver `c9017b8`, with no server changes in this milestone.

Inspected before implementation on 2026-10-19. The user-specified `/Users/test/dev/reaver` checkout is at `04641815e49ccafb032b94d0ba59b02529273a8d` (`ic-cells`). Its HTML.md describes prepared geometry, but it has no `extras/mounts/goo/PORTABLE.md`. The user explicitly selected `/Users/test/dev/reaver-two` for the integration server after this discrepancy was reported. Neither upstream repository was edited.

The implemented baseline is `reaver-two` HEAD `a1a2bb4df523419cb225f9431eab03b06e997534` **plus its existing working-tree changes**. UIKit is HEAD `e1dbdfde6dd5870d85b41cfdef0d67e615ecd721` plus its existing consumer. HEAD alone is insufficient to reproduce the portable additions. `evidence/provenance.json` records hashes of the actual inspected source and copied fixtures/assets.

Sources inspected:

- Reaver `extras/mounts/goo/{PORTABLE.md,HTML.md,prepared.foil}` and pinned `profile/`.
- Explorer `goo-actions.js`, `goo_browser.foil`, pane/session code/contracts and actual compiled deck/source fixtures.
- UIKit `GOO_RENDERER_SPEC.md`, `GOO_PARITY_ROADMAP.md`, README, `GooModels`, `GooDescriptors`, `GooProfile`, `GooPainter`, `GooSurfaceView`, `GooClient`, `GooEditorOwner`, `GooWorkspaceModels`, `GooDeckSession`, tests and demo mount.

The renderer specification and roadmap contain proposals. Capability negotiation, a reusable cross-language kernel, general pane composition, complete native host workflows, and full portable-patch consumption are not implied by their descriptions. The current server does implement version-1 paint/semantics, finalized bindings/captures, typed patch content, and trusted host descriptors. This consumer implements only the advertised subset.

| Implemented wire contract | Rust behavior |
|---|---|
| POST `/goo/render`, form `target`, `slot` or `view`, `width`, `textScale`, `direction` | Uses `view` for declared pane, `slot` for ordinary follower; validates exact request context. |
| `display.version:1`, `absolute-css-px`, profile id/version/revision, font id/SHA256 | Reject unsupported identities before mounting; use bundled matching font bytes. |
| Absolute boxes, lines, `textX`, baseline, `prefixAdvance` | Paint supplied positions; glyph advances use the pinned font with kerning/ligatures absent. No client wrapping, Goo layout, or role palette. |
| `textStyle.insetX/insetY` | **Total** horizontal/vertical padding, divided by two per edge (confirmed by UIKit's native field adapter). |
| `paint.version:1`, `semantics.version:1` | Execute supported recipes and expose supplied meanings; unsupported vocabulary rejects the preparation. |
| Finalized `binding` and legacy flat fields | Require agreement, unique IDs/slots, preorder node parents, node-kind joins and presented flags. Hidden metadata creates no widgets; button/textbox meaning grants no authority. |
| GET `/goo/source?target=...&axis=...` | Validate bounded direct children, unique keys, source life, member life/type/axis and status. Same path with a new life does not inherit an editor. |
| POST `/goo/act` | Freeze `target` (presented subject), `subject` (snapshot receiver target), `view`, finalized `control` route, `expected`, `displayed`, `publication`, `invocation`, and all `input:*` values. |
| Capture reference `{slot,selection}` | Resolve in the pane's own scope and freeze selection identity/incarnation/revision plus member/source lifetime and expected evidence. Missing captures disable activation and fail capture. |
| GET `/goo/operation-status` | Require matching operation ID; accepted/rejected terminal outcomes distinct from unknown. Missing/malformed/uncertain outcomes remain recoverable. |
| Deck keys and lexical selection keys | Registry retains owner UUIDs through reorder; removal/recreation creates fresh UUIDs. Pending event delivery uses editor UUID and render generation or invocation ID. |

`src/model.rs` is admission; `src/transport.rs` owns immutable invocations; `src/session.rs` owns work and structural transitions; `src/painter.rs` executes prepared geometry; `src/main.rs` projects session state and routes worker completions. Worker closures never read the currently visible editor to decide a completion target.

Unsupported trees, patches and host workflows must later consume `topology`/source contracts, `contentVersion:1`/typed occurrence content and offered bindings, and trusted `host` descriptors respectively. HTML fields are never parsed. Unknown versions remain unavailable until implemented and verified.

## Main-based server port (2026-10-19)

The portable additions were subsequently ported to `/Users/test/dev/reaver`, on
`lf/goo-contracts` based on main `f373423`, committed as `fd7ef90`. The initial inspection provenance above
remains historical. New disposable runs default to the sibling `reaver` checkout.
Ordinary paint, semantics, binding, and pinned profile identities are unchanged.
Main adds stored workspace sessions; the port emits `workspace-session` navigation
with `sourceSlot`, implemented by the subsequent Explorer milestone below. Portable
patch content also covers server-generated `select`, embedded `presentation`, and
dynamic populations; those remain explicitly outside this client's supported slice.

The server's `recipePublication` is retained in the inspector's original response.
Editor lifetime currently still uses global publication, so unrelated publication
changes may conservatively trigger replacement guards. Action admission continues
to use global publication.


## Native namespace Explorer (2026-10-19)

The current Inspector declaration is consumed without rewriting it. `--explorer`
adds a recursive semantic owner registry for pane/split/stack, lexical tree
selection ports, topoi, followers and their prepared fallback, and native nodes.
Ordinary preparations continue through the same admission, editor, painter and
frozen-invocation implementation used by the original bounded deck.

Two server additions close observed gaps: configured workspace entry supports
`format=json` and returns its stored session/declaration/host; writable synthetic
Values exposes `readOnlyDisplay` alongside its unchanged browser HTML editor.
The latter is descriptive read-only content, not an editing capability.

Tree paging consumes `incremental`, `parent`, `next`, `epoch`, `omitted`, membership
life/evidence, and prepared `pane-v1` row bundles. Every packet and all three variants
are validated before installing a page. Parent/source/lifetime/epoch mismatches,
duplicate members or packets, and missing available-row preparations reject the
whole candidate. Materialization is limited to 4096 nodes. Existing rows survive
failed page fetches. Collapse retains owners and loaded pages. Refresh rebuilds
previously loaded branches against one new epoch and reconciles member lives.

A structural response must match the initiating intent and work revisions before
it can even become a guard candidate. Retirement checks affected owner UUIDs and
revisions again at commit. Compatible preparations merge working drafts and pending
invocations; replacement lifetimes never inherit them. Save completion resolves its
UUID, invalidates old row bundles, rereads the originating surface, then refreshes
source evidence. Recipe picker changes are guarded host transitions; `become`
bindings remain unsupported.

`mount-rescan` follows status -> rescan with expected scan -> poll exact requested
scan -> successful publication -> staged workspace refresh. Dirty/composing or
unresolved editors refuse publication. An error retains the installed workspace
and exposes status checking; status checking never sends another rescan.

Scope remains the namespace Explorer milestone: Values inspection plus ordinary
view editing. Arbitrary raw slot editing, diagram trees, patches, generic deck
composition in Explorer mode, arbitrary `or` branches, RTL/text scale and unsupported
paint primitives remain explicit boundaries. No HTML-derived behavior is used.


## Native surface expansion / milestone 2 (2026-10-19)

Re-inspected the implemented portable encoder, patch-demo, shared panes declarations,
Explorer's action adapter, and Mash's patch scene vocabulary before extending the
consumer. No roadmap proposal was treated as an endpoint. The current surface code
is `src/patch.rs`, `src/diagram.rs`, `src/paint_geometry.rs`, `src/images.rs` and the
existing painter/session/Explorer modules. No HTML is parsed.

Patch admission joins occurrence keys/lifetimes, endpoints, ports, route parts,
labels, descriptor content and offered controls. Complete ordering comes from
`snapshot.projectionSeq`, not a client-invented counter or scene layout revision.
Explicit `coveredOperationIds` retires optimistic move previews. The four offers
join real presented invoke controls and their argument inputs; descriptions and
passive selection never create action authority. Gesture start freezes the scene,
and release requires it still to be current. Frozen invocations then use ordinary
Goo retry/status semantics. Editors are scoped to body routes plus occurrence
lifetimes; embedded references add their own subject and recipe identity. Reordered
occurrences retain owners; replacements cannot inherit working state. Pending
completion checks both owner UUID and invocation ID. Compatible projection refresh
preserves camera, selections and drafts; incompatible changes preserve old state.

Diagram arrangement owns only graph topology positions and links. Each node's
content width/height comes from its admitted prepared display. It does not rebuild
Goo wrapping or layout. The arrangement differs from pinned browser d3-flextree.

Ordinary recipe changes are staged through the Explorer guard. An invoke-and-become
freezes its destination with its invocation; rejection/unknown cannot navigate.
Accepted save completion and later-draft departure approval are separate decisions.
Patch embedded/body recipe transitions and the original demo's recipe-changing
views are rejected explicitly until their transition coordinator is implemented.

The prepared painter now consumes supplied gradient stops/angles, marker metrics,
image cover geometry, progress semantics and shape clipping. SVG images are decoded
without external resources. Source reads and decoded dimensions are bounded.
Text-scale/direction are request context and must match the returned preparation;
there is still no Unicode shaping contract or client role palette. AccessKit exposes
image/progress roles, ranges and named scene controls, with the certification limits
recorded in VERIFICATION.md. Generic panes/decks, query/create/reveal offers and
unsupported occurrence arrangements remain explicit admission boundaries.

## Patch hardening implementation findings

- Current Reaver rejects `patch_presentation` as a static Grove constructor. Its implemented portable reference contract is emitted from typed `goo_patch/population` native nodes, as in `goo-moodboard/scene.foil`. The disposable embedded fixture follows that path.
- Reference `rect` is relative to the containing node or connection label. The Rust painter now adds that supplied occurrence origin (and current move preview) before applying the camera transform. The reference frame owns native scrolling and retains its editor/slot identities.
- macOS can coalesce press, movement and release into one egui frame, with no `drag_started`/`drag_stopped`. Handle capture must use the press position and consume the release position; movement stays a preview until the validated offered operation is frozen and sent.
- Prepared response completions are not user edits. During an Explorer workspace stage, patch preparation events are deferred, while action outcomes continue reaching their originating owners. This avoids treating an initial embedded mount as superseding user work; existing work-revision guards still protect real edits and save outcomes.
