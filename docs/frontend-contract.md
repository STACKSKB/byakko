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

A CLI can construct the same session and executor and expose discovery,
capability inspection, read-only state export, draft review and explicit apply.
It should show the target identity and operation result, preserve opaque values,
and use the same expected-state check, durable backup, readback and typed
recovery outcome as the desktop. CLI flags are presentation; they must not grow
a second protocol implementation or bypass the session. This is the first
useful proof of the contract after the current desktop acceptance work.

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

Do not introduce a universal report interpreter, remote service, plugin ABI or
frontend framework merely to prove that the seam exists. The next concrete
test is a small CLI using the current read path; adapt the contract only where
that client exposes a real gap.
