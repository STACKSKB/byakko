# Iced macro slice, 2026-09-22

The native composition supplies a core `Session` with Nia87 capabilities and
the existing device executor. `--demo` supplies the same UI/session with three
named macro slots, distinct limits, and an opaque fixture. Neither view nor
input controller imports the Nia87 adapter or legacy GUI.

Implemented: select/read slot, inspect and replace events, append, reorder,
remove, clear, edit stored repeat count, revert, explicit Save & verify, and
stage a saved macro binding onto the selected key/layer.
Action choices and ranges come from capabilities. Keyboard usages and pointer
movement are numeric inputs in this pre-alpha; pointer buttons and backend
actions use supplied labels. Wait is after the event. Zero remains an explicit
value; the UI does not claim zero repeats means infinite playback.

The core owns baseline/draft, validation and trust. The desktop only owns
unsubmitted field text and an optional replacement index, cleared when the
sequence changes. Stage is local; Save uses the existing expected-state,
backup, write, readback and typed recovery path. Unknown slots are read-only.
Failure/conflict keeps the core draft; failed reads also retain form text.
Closing waits for any pending command, leaves failures visible and considers
dirty drafts across both pages before offering discard.

Binding choices and repeat requirements come from backend capabilities. Nia87
offers counted, toggle and hold; toggle/hold require saved count 1. A visible
Stage count action changes only the macro draft. Save it, read keymaps, reread
the slot, then stage the binding and review/apply it on Keys. The rereads retain
the conservative cross-feature trust invalidation required by the earlier
hardware recovery anomaly. This sequence does not claim an atomic transaction
across keymap and macro data. Updating a shared slot can affect existing keys.

The demo uses named binding actions rather than numeric Nia87 encodings. Its
binding workflow test rejects wrong/unsaved repeat counts and fixed keys,
preserves other drafts, explicitly saves the required count, rereads, stages
a third-layer binding, applies it through the memory executor and verifies
the resulting keymap and macro. Native capability fixtures check the known
four-byte bindings and count policy at the first and last slots.

Validation: 217 workspace library tests pass. New desktop tests run messages
through the memory executor, verify saved values by rereading, preserve zero
wait/count and signed motion, reject an out-of-range edit atomically, reject
opaque writes, and check close/failure/stale-result behavior. Form projections
round-trip every demo action kind. Windows and Linux all-target/all-feature
Clippy pass with warnings denied. Windows release builds.

This is not macro feature parity. Recording, file import/export and local labels
remain in the legacy application.
No hardware writes were performed for this UI step. Rendered interaction,
physical playback, Linux runtime and power-cycle persistence remain unverified;
the screenshot helper failure documented in `iced-keymap-acceptance.md` remains
open. The earlier injected-failure recovery problem is also still open.
