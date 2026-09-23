# Frontend contract and delivery targets

The native Iced app is one client of a Rust application contract, not the owner
of keyboard semantics. A CLI and a browser client should use the same device
descriptors, edits, commands, completions, and safety transitions. This is a
source-level Rust contract today, not a promised stable network or plugin ABI.

## The seam that exists now

`byakko-core` owns `Descriptor`, `PhysicalKey`, `Action`, feature capabilities,
snapshots, drafts, validation and the `Session` transition rules. Its
`Command` and `Completion` values are owned and serializable; generation and
operation IDs reject stale results. No core value imports Iced, HID, file paths,
channels, clocks or operating-system types. A frontend calls session methods to
stage and request work, then feeds a completion back to the same session.

`byakko-devices` supplies concrete backends and the serialized executor. It
owns discovery, exact HID target selection, protocol framing, backups, pacing,
write verification and recovery. The Nia87 profile supplies board identity and
physical geometry; generic views do not encode its matrix or report IDs. Other
backends, starting with QMK/VIA, should implement the exercised device
interface and publish their own capabilities. A Wacom-like external target can
share the physical-key selector and HID inventory logic without acquiring
keyboard configuration commands it does not support.

`byakko-desktop` composes those layers and renders session projections. Iced
messages, pane layout, text input and local window state stop at this boundary.
The current binary is a Nia87 composition root; it is not a mandatory frontend
for the library crates.

## CLI slice

`byakko-cli` is the first independent client. `devices` checks exact Nia87
configuration-interface availability, `describe` prints its physical keymap
descriptor, `read` exports a verified USB keymap as JSON, and `read-colors`
exports the per-key color snapshot after the keymap read. `read-lighting` and
`read-settings` likewise export typed snapshots and preserve raw revisions.
`read-macro <slot-id>` reads one slot advertised by the backend's capabilities
and exports the complete snapshot, including opaque revision bytes, without
decoding backend-owned content in the CLI. `capture-archive <new-file>` saves a
complete opaque native backup only after the backend's verified capture; it
refuses to overwrite an existing path. `review-archive <file>` reads that
wrapper, captures the device again, and prints only the changed section
summaries; it sends no setter. `read` output is also a complete keymap state
file. After editing its bindings, `plan-keymap <state-file>` compares it with a
fresh device read and reports the intended changes. `apply-keymap <state-file>`
requires the exact raw before-image revision from that fresh read and passes the
changes through the Nia87 adapter, then stages them through the same
`Session → Command → Executor → Completion` path as Iced. The device backend
creates a durable backup, performs the write and checks complete readback; the
CLI waits for its outcome rather than abandoning an in-flight write on a read
timeout. The Nia87 adapter rejects new opaque bindings and unadvertised key or
shortcut usages while preserving opaque baseline values. Memory-device tests
exercise planning, apply and stale-file rejection without USB. No physical CLI
write has been attempted yet.

`read-settings` output is likewise an editable snapshot file.
`plan-settings <snapshot-file>` compares one proposed field with a fresh USB
read and sends no setter. `apply-settings <snapshot-file>` accepts exactly one
changed field per invocation, verifies its complete catalog and raw revision,
then uses the same session/executor transaction as Iced. The backend checks
the complete native settings readback and keeps a durable before-image.
Memory-device tests cover the write, reread and rejection paths. The attached
Nia87 has exercised planning only; physical CLI settings apply remains open.

`read-lighting` output can be edited as a complete global-lighting snapshot.
`plan-lighting <snapshot-file>` reads the device again, checks the exact raw
revision and backend, validates the desired effect through the portable
catalog and Nia87 preflight, then prints the desired setting or `null` for no
change. `apply-lighting <snapshot-file>` requires a change and uses the same
guarded session/executor transaction as Iced. The attached board has exercised
planning only; physical CLI lighting apply remains open.

`read-macro <slot-id>` output is a backend snapshot with the complete raw
before-image. `plan-macro <snapshot-file>` freshly reads that exact slot,
checks its backend and revision, validates the proposed program against the
portable capabilities and Nia87 codec, then displays full before/after
programs. `apply-macro <snapshot-file>` stages through the same correlated
macro-file ticket and guarded executor path as Iced. A stored repeat count of
zero remains readable and can plan as an unchanged no-op, but a changed
program must use the advertised editable count range. This snapshot workflow
is distinct from the portable macro document format, whose source slot is
descriptive rather than an expected-state token. Physical CLI macro apply and
playback remain open.

Later CLI work can expose archive apply workflows. It should
show the target identity and operation result, preserve opaque values, and use
the same expected-state check, durable backup, readback and typed recovery
outcome as the desktop. CLI flags are presentation; they must not grow a second
protocol implementation or bypass the session.

## Browser slice

A future static SPA can compile the portable Rust model to WebAssembly and use
a small browser adapter for rendering and device effects. It need not run a
Node.js server on the user's machine. Browser JavaScript/WASM bootstrap code
would belong only to that optional web client; the native driver and Iced GUI
remain JavaScript-free. The web adapter must implement the browser's actual
device-access and permission rules rather than promise the native app's
automatic plug-and-play behavior. A native backend exposed through a separate,
deliberately secured service is another possible transport, not part of this
slice. Keep command semantics the same whichever transport is chosen.
Chromium's [WebHID access model](https://developer.chrome.com/docs/capabilities/hid)
requires an explicit device permission flow and may protect keyboard HID
collections; a static build alone cannot remove those constraints.

Do not introduce a universal report interpreter, remote service, plugin ABI or
frontend framework merely to prove that the seam exists. Adapt the contract
only where an independent client exposes a real gap.
