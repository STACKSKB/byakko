# Byakko rewrite plan

Started 2026-09-26 on `codex/application-rewrite`, from `3db624f`.

## Objective and authority

Replace the accumulated application with a human-readable, modular program.
A feature should be understandable locally: data, legal edits, effects, accepted
outcomes and presentation. Favor simple functional composition and explicit
ownership over guards, wrappers and synchronized controller state. The user's
Elm/Xmonad examples express this clarity; Rust and Iced remain.

Remove the old application rather than keeping a legacy frontend beside the new
one. Git preserves it. Selectively carry forward reviewed portable domain values,
firmware codecs, native transactions, useful research tools and evidence. These
are protocol/domain assets, not a reason to preserve old controller ownership.
Temporary construction gaps do not redefine the full configurator objective.

Read [AGENTS.md](AGENTS.md) and [the constraint inventory](docs/rewrite-constraints.md).
Historical hardware acceptance describes the old executable and particular unit;
it is not evidence that new frontend paths work.

## Architectural decisions

1. Four product crates: core, devices, desktop, CLI. Root research tools depend
   inward. Do not add packages merely to impose another privacy layer.
2. Core `contract` owns serializable commands/completions, IDs and outcomes,
   separate from `session`. Native queues are adapters, not the contract.
3. Connection, feature state and operation identity are separate. Keymaps have
   an editor; keymap readiness is not connection state.
4. Feature owners handle validation and accepted observations. Session owns
   exclusivity/correlation and genuine cross-feature effects. Named workflows
   compose features. No giant controller hidden in multiple `impl` files.
5. Core acceptance returns typed facts. Frontends must not reconstruct success
   from every editor's status. Preserve Readback versus TransportAccepted.
6. Desktop owns widget text, selection, gestures, navigation and window lifecycle.
   Baselines/drafts remain in core. Pass specific inputs instead of global Desktop.
7. Auto-save distinguishes latest intent from submitted target. Outer timers
   deliver deadlines; feature owners coalesce. Settings save one scalar at a time.
8. One immutable selected-device worker executes effects. Foreground work takes
   priority between passive macro slot reads; host frames are bounded. Keep known
   firmware settling time, not incidental delays from extra identity reads.
9. Native modules own conversion/planning/transactions by feature. Collapse pure
   forwarding layers. Replace error downcasting with typed transaction failures.
10. Use higher-order functions for demonstrated shared transformations/steps.
    Prefer explicit enums/functions when clearer. No speculative frameworks,
    service hierarchies, blanket cloning or metric-driven mechanical splitting.
11. Capabilities drive UI and constraints; preserve backend revisions and opaque
    bytes. Shared UI/core do not assume Nia87 slots, layers or report widths.
12. Preserve interaction behavior. Temporary feature gaps are recorded below;
    they do not authorize a reduced final product or deletion of firmware evidence.

## Intended ownership

| Area | Responsibility |
| --- | --- |
| core/contract | Commands, completions, IDs, evidence and failures |
| core/session | Connection, exclusivity, correlation and routing |
| core/keymap, macros, lighting, picture, settings | Models, edits, transitions, projections |
| core/workflows | Genuine multi-feature sequences such as save-and-assign |
| devices/executor | Serialized delivery and scheduling |
| devices/hid, rongyuan, nia87 | OS collections, family codecs, board mapping, transactions |
| devices/storage and sampling | Durable files and OS samplers |
| desktop/app and workspace | Iced lifecycle, routing and persistent keyboard layout |
| desktop/features and widgets | Local forms, feature messages/views and shared controls |
| CLI | Typed parsing, file intent and output over shared core |

Use subdirectories only where navigation benefits; do not manufacture files just
to match this table. Public APIs expose concepts, not every internal helper.

## Implementation sequence

- [x] Inspect checkout and create rewrite branch.
- [x] Replace engineering rules and write this plan.
- [x] Review detailed constraint inventory against previous rules and current
  macro encoding/pacing, including the 250-byte upload versus 256-byte snapshot.
- [x] Replace old session, desktop/CLI controllers and worker with working keymap
  read/edit/save/revert/reconnect slice. Include retained drafts, close behavior,
  typed completion and deliberately different memory device. No live writes.
- [ ] Rebuild macro library/editor/recording and pure save-and-assign workflow.
  Preserve unknown slots, candidate reads, count rules and partial outcomes.
- [ ] Rebuild coalesced lighting, picture and scalar settings with owned intent
  and submission state, real selector dependencies and one-pass read policy.
- [ ] Restore discovery, files, local labels and diagnostic capture through the
  new architecture. Developer archive restore remains outside public UI.
- [ ] Restore host lighting lifecycle, OS samplers and verified restoration.
- [ ] Remove remaining obsolete native forwarding/error-erasure paths, retaining
  fixtures and useful research commands. Narrow APIs around demonstrated callers.
- [ ] Review ownership/docs, run workspace checks, compare protocol sequences
  and record outstanding physical gates.

## Validation

Each integrated slice must build both clients and exercise real public intents
using the memory backend. Verify rejected/stale/wrong-direction completions,
unchanged drafts on failure, newer queued intent during writes, reconnect
retention, partial workflow outcomes and closing/restoration. Preserve tests for
exact encoding, cached before-image backup, report order, pacing and read counts.

Run `cargo fmt --all -- --check`, workspace tests and strict Clippy after coherent
integration. Do not invent frontend handlers only for tests. Physical output,
rendered layout and Linux/hardware acceptance require their own coordinated work.
Do not claim a rewrite fixes the recorded recovery failures.

## Current status

The user explicitly confirmed source replacement on 2026-09-26, superseding the
audit-only restriction. The old core session workflows, desktop controllers, CLI
controllers and host worker scheduler are removed. The replacement keymap path
has a separate connection state, keymap editor and typed outcomes; both clients
use the new session and finite-command executor. The shared simulator has three
keys, three named layers and a read-only key. Native wire behavior is retained.

Integration review is complete. Workspace library/binary/integration tests,
strict all-target/all-feature Clippy, formatting, and locked native development
builds pass. The built CLI completed demo read -> plan-keymap -> apply-keymap
against memory. Review caught and fixed file apply accidentally including an
unrelated existing draft; it now rejects that situation without changing edits.
The executor test counts one initial read and one apply, with no extra read.

Native transaction diff review found only type-namespace/import changes; report
bytes, HID selection, pacing, backup and recovery logic are unchanged. No live
device writes were performed. New UI rendering, physical output and Linux runtime
behavior have not been accepted.

Commands run at this checkpoint:

```text
cargo test --workspace --all-features --offline --lib --bins --tests
cargo test -p byakko-cli --offline
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check
cargo build --locked --offline -p byakko-desktop -p byakko-cli
target/debug/byakko-cli --demo read
target/debug/byakko-cli --demo plan-keymap target/rewrite-smoke/keymap.json
target/debug/byakko-cli --demo apply-keymap target/rewrite-smoke/keymap.json
```

Temporary gaps: automatic discovery/reconnect, the full assignment catalog and
custom shortcut form, macros/recording/save-and-assign, coalesced lighting/picture/
settings, host streaming, local labels/file workflows and diagnostic capture.
The native APIs, codecs, OS samplers, direct transaction tests and research tools
remain. Passive catalog priority and host worker scheduling return with their
feature milestones; a finite catalog command is not passive background discovery.
The full rewrite remains incomplete; this checkpoint is not configurator parity.
