# Goo client contracts

This is the current implementation reference for the Rust/egui consumer.
[README.md](README.md) contains setup and feature walkthroughs;
[capabilities.json](capabilities.json) is a descriptive inventory, not a runtime
negotiation protocol. [VERIFICATION.md](VERIFICATION.md) is a dated verification
history: an older milestone's unsupported list does not override this reference.

## Server compatibility

The server lives in a separate Reaver checkout. The runners default to the sibling
`../reaver`; `--reaver /path/to/reaver` overrides it. A compatible checkout needs:

- `x/eden` and the Reaver runtime/Nix dependencies used by that checkout.
- `extras/mounts/goo/protocol/goo.proto` with domain **protobuf v2** and the matching
  encoder/endpoints. The rejected generic v1 protocol is not supported.
- The pinned `mash-goo` profile and font bytes matching this client's
  `assets/profile.json` and `assets/run-fonts.json`.
- For Explorer: workspace-session JSON entry, trusted host declarations,
  paged tree sources with `pane-v1` row bundles, and prepared `readOnlyDisplay`
  Values fallback in `extras/mounts/explorer`.
- For styled/rich input: prepared display v2/v3, actual `goo/runt` myth metadata,
  `goo/text/pail`, and the `goo_edit/decode` typed action codec used by the fixture.
- For optional demos: `extras/mounts/patch-demo`, `extras/tests/fixtures/panes`
  and/or `extras/tests/fixtures/tree-pages`; pipeline inspection needs the server's
  opt-in capture implementation.

The repository does **not** pin a complete server revision for all current
features. Reaver `fd7ef90` and `c9017b8` in the historical provenance describe the
original main-based port and Explorer milestone, before the later text, pipeline
and protobuf work. Do not treat either as sufficient for today's default client.
The runner checks for `x/eden`, not the full compatibility list. Record the actual
server revision and working-tree changes when reproducing results.

## Versions and admission

The outer presentation envelope and profile are version 1. `display.version` is
independent: **1** for ordinary text, **2** for resolved styled runs, **3** for
rich-input editing contracts. Paint, semantics, finalized bindings and the rich
`editing` contract use version 1; rich wire strings use `goo-text-v1`.

`src/model.rs` validates the requested subject, selector/context, profile identity,
font IDs and SHA-256 hashes, finite geometry, preorder parents, unique node/input
identities, text/style recipes, and binding joins before mounting. Context width
must be positive and below 10,000 CSS px; text scale is 0.25–4 inclusive and direction
is LTR or RTL. Explorer UI presets are 1×/1.5×/2×. Unsupported versions, descriptors
and surface arrangements fail admission rather than acquiring guessed behavior.

Finalized bindings must agree with legacy flat fields and presented nodes.
Semantics, links and passive selection do not create invocation authority. Ordinary
view validation rejects host descriptors; Explorer validates its workspace host
and declaration separately. Accepted host operations are configured `mount-rescan`
operations with `workspace-session` navigation.

## Prepared rendering and native editing

Static content uses supplied absolute CSS-pixel boxes, lines, `textX`, baselines,
`prefixAdvance`, font metrics and normal/state paint. There is no client Goo
wrapping or layout pass. `textStyle.insetX/insetY` are total horizontal/vertical
padding, divided by two per edge. One CSS px is one egui logical point.

Supported primitives are group, text, code, button, input, image/media/avatar/
presence and progress bar. Paint supports solid/linear-gradient fills, inside
borders, radius/ellipse backgrounds, inherited content clipping and supplied
circle status markers. Image sources may be data, HTTP(S) or server-relative paths;
PNG/JPEG/GIF/WebP/SVG are decoded asynchronously. Media is image paint, not a video
player. Reads are bounded to 8 MiB; decoded dimensions to 4096 per axis and
4,194,304 pixels. SVG decoding uses no external resources.

Native plain inputs use single-line egui TextEdit. Display-v3 rich inputs use
multiline TextEdit with toolkit editing layout and supplied style recipes; their
working lines are not the static server-prepared lines. Caret, selection, focus,
clipboard, IME events and AccessKit semantics are integrated. Hardware IME and
VoiceOver operation remain uncertified. The docs/history contain no recorded
comparison or selection rationale between egui and GPUI Component.

Text preparation/submission is English/ASCII. Runs carry the actual Foil `myth`
type, projected as branded typed values at the wire boundary. Implemented style
slots are `/sys/font`, `/sys/modifiers` (bold/italic/underline) and `/sys/link`
(internal path or weblink). Fonts are bundled pinned sans weights/italics and
regular/bold mono. Link targets remain descriptive; they do not navigate.
Arbitrary fonts, mono italic and Unicode shaping/bidi are unsupported.

Rich inputs require an explicit `editing` contract with a nonempty allowed-style
palette (at most 256 entries), link recipe, 65,536-byte encoded-value limit and at
most 10,000 runs. Formatting, typing, selection and structured undo/redo belong to
the editor owner; composition is grouped into one undo transaction. Undo retention
is bounded to 100 snapshots and approximately 65,536 stored characters, retaining
at least one snapshot. Clipboard interchange is plain text. Unsupported characters
remain in the draft and block submission. A declared typed action codec must call
`goo_edit/decode` to turn the frozen JSON wire string into stored `goo/text/pail`;
the ordinary `.poke` string shortcut does not perform that conversion.

## Ownership, structure and action recovery

`src/session.rs` owns drafts, generations, invocations and structural transitions.
Owners have UUIDs independent of widgets. Compatible refresh/resize and keyed
reorder preserve drafts and editing identity. Recreated pane/member/occurrence
lifetimes receive fresh owners. Completions resolve the originating UUID plus
render generation or invocation ID, including when that owner is hidden.

Structural changes validate and stage replacements before retiring installed
owners. Initiating intent and work revisions must still match when staging and
committing. Dirty departures offer Stay/Discard; unresolved saves and composition
block retirement. New edits can supersede an old departure guard. Suspension
retains drafts, disables actions and requires fresh preparation on resume.
Global publication still participates in editor lifetime and action admission;
unrelated publications can conservatively require replacement guards.

`src/transport.rs` captures an invocation once: `target` (presented subject),
`subject` (snapshot receiver target), `view`, finalized `control` route, `expected`,
`displayed`, `publication`, invocation UUID, wire selector and all `input:*` values.
Selection captures add lexical selection identity/incarnation/revision plus
member/source lifetime and evidence. Missing captures disable activation. Rich
inputs are validated and frozen as complete structured-run JSON strings.

Accepted, rejected and unknown outcomes remain distinct. Timeout/malformed/uncertain
replies retain the original invocation for `/goo/operation-status` or an exact
retry with the same ID and bytes. Late completion does not clear newer typing.
Recovery is in memory only: there is no authentication adapter, offline queue,
detached operation store or process-termination recovery. Window close is guarded
while work is dirty/unresolved; forced termination loses that recovery state.

## Surface-specific boundaries

| Surface | Implemented contract | Limits |
|---|---|---|
| Standalone/deck | Ordinary prepared editors; up to eight keyed panes, each with one direct-child list/follower and at most 64 members | No recipe-changing controls in this mode |
| Explorer | Recursive pane/split/stack, lexical tree selections, list/diagram topoi, follower/native fallback, navigation and mount status/rescan | At most 512 workspace components and 4096 materialized tree nodes; only the declared follow/native `or` alternative; no general decks or raw Values editor |
| Tree pages | Epoch-bound continuations, source/member life/evidence and all `pane-v1` sm/md/lg row preparations validated before installation | Up to 64 children per page (root may additionally include its source member); failed admission retains installed rows; collapse retains loaded rows and owners |
| Diagram | Shared tree selection and prepared node content sizes; host supplies topology positions/links | Host arrangement differs from browser d3-flextree |
| Patch | `contentVersion:1`, one scene through supported wrappers; prepared bodies, ports, labels, embedded ordinary references and occurrence lifetimes | Up to 4096 nodes/8192 connections; only moveNodes/connect/reconnect/disconnect; no query/create/reveal or arbitrary nested occurrence arrangements |
| Recipe changes | Ordinary Explorer `become` stages a guarded preparation; `invoke-and-become` freezes its destination and stages only after acceptance | Unknown/rejected saves do not navigate; later typing can still refuse departure; patch-body/embedded transitions are rejected |

Patch admission joins offers to presented ordinary invoke controls and argument
inputs. Ordering uses `snapshot.projectionSeq`; explicit `coveredOperationIds`
retire move previews. Gesture start freezes the scene; release must still match
it. Capture consumes coalesced press/move/release events and cancels on Escape or
suspension. Compatible projections retain camera, selection and drafts; incoming
projections cannot silently replace an active gesture. Embedded reference frames
add the containing node/label origin and own native scrolling/editor identity.
Preparation completions are deferred during workspace staging while action
outcomes continue to reach their owners. Connection picking uses route bounds,
and preview wires retain supplied projection geometry.

Mount rescan uses status → rescan with expected scan → poll the exact scan →
successful publication → staged workspace refresh. Dirty/composing/unresolved
editors refuse publication. Uncertain status does not automatically issue another
rescan; failed updates preserve the installed workspace.

## Transport

Ordinary prepared views/panes, source pages, status snapshots and action receipts
request `wire=protobuf-v2`. Domain messages carry binary64 geometry and typed
text/style/paint/myth records. Successful binary replies require
`application/x-protobuf`. Protobuf v1 is rejected. Requests, including frozen action
bodies, remain URL-encoded; receipt persistence is unchanged.

JSON remains explicit for workspace bootstrap/declarations, pipeline captures,
patch packets, all-views catalogs and inspection capture metadata. Ordinary
`Client::render` may repeat a read as JSON only after a typed HTTP 415 failure with
`code=json-contract` and `error=Goo JSON-only contract` (for a patch result).
Arbitrary failures do not trigger format fallback. Actions and receipt retries
never downgrade or recapture. Pipeline requests use JSON directly.

Ordinary presentations/sources deserialize into Rust models and retain received
protobuf bytes. Inspector JSON is generated on demand; Explorer structural
consumers also use decoded value projections. A decoded diagnostic projection is
not the original JSON response body. Small server paint/semantics/binding adapters
and pane/tree APIs still originate in JSON-returning APIs; this is not a claim
that every server path is free of JSON adapters.

HTTP responses and protobuf packets are bounded to 8 MiB. `Client::new` uses a
20-second HTTP timeout and a separate 120-second preparation timeout. Ordinary
renders and requests through `explorer_bytes` use the latter; ordinary source,
action submission and `Client::status` receipt lookup use the former.
`with_timeout` overrides both. Worker threads use bounded deadlines and stale-ticket
rejection, not transport cancellation.

## Inspection and pipeline capture

Inspection is read-only and frozen to owner, mounted generation, target, recipe
and publication. JSON responses retain unknown fields; protobuf retains original
bytes and exposes a decoded JSON projection. Drafts never replace received seeds.
Rich-input right-click opens formatting; other prepared components offer received
Goo inspection. The toolbar can open the inspector independently.

Opt-in `pipeline=1` attaches stages to the same prepared snapshot. The visual
preview uses the production painter without editable widgets or invocation targets.
Stage overlays join explicit IDs and supplied geometry; source/core before IDs,
publication-time parse/read and HTML do not acquire invented geometric mappings.
Nested fragments retain independent coordinates. Parse/read may be skipped; HTML
is literal text. Failed compilation history, unmounted staged candidates, full
host-scene recomposition and large-scene performance are outside this inspector.

## Historical provenance

The initial 2026-10-19 audit used Reaver-two `a1a2bb4` **plus existing working-tree
changes**, and UIKit `e1dbdfd` plus its existing consumer. HEAD alone did not capture
the portable additions. `evidence/provenance.json` records inspected source hashes.
The later main-based port was `fd7ef90` on base `f373423`; Explorer was `c9017b8`.
The corresponding `evidence/*provenance.json` files describe those historical
milestones, not a pin for current rich-input or protobuf-v2 support.

Early audits deferred trees, patches, richer paint and recipe changes; subsequent
milestones implemented the bounded contracts above. The chronological results
and residual verification limits remain in [VERIFICATION.md](VERIFICATION.md).
Renderer specifications/roadmaps are proposals unless backed by the implemented
contracts; full browser parity and general native host workflows are not implied.
