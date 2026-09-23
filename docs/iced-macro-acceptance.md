# Iced macro slice, 2026-09-22

The native composition supplies a core `Session` with Nia87 capabilities and
the existing device executor. `--demo` supplies the same UI/session with four
named macro slots, including one vacant slot, distinct limits, and an opaque fixture. Neither view nor
input controller imports the Nia87 adapter or legacy GUI.

Implemented: select/read slot, inspect and replace events, append, reorder,
remove, clear, edit stored repeat count, revert, explicit Save & verify, and
stage a saved macro binding onto the selected key/layer, focused recording,
and bounded local JSON import/export.
Opening Macros reads every advertised slot once through a serialized,
backend-neutral catalog operation after the keymap is ready. The library shows
stored programs and slots referenced by either the verified or staged keymap;
an empty bound slot cannot be allocated. Add reads the first free slot and
disables at the advertised capacity. Selecting a library entry reads that
slot's editable snapshot. Failed reads require an explicit retry. Memory
tests cover the catalog, Add, full capacity and pending-keymap behavior.
The grid reserves layout space for its scrollbar.
Action choices and ranges come from capabilities. Keyboard usages and pointer
movement are numeric inputs in this pre-alpha; pointer buttons and backend
actions use supplied labels. Wait is after the event. Zero waits remain
explicit. A stored zero repeat count is preserved, but the Nia87 editor now
requires 1–65,535 before a new save or binding; zero playback semantics remain
unknown.

The core owns baseline/draft, validation and trust. The desktop only owns
unsubmitted field text and an optional replacement index, cleared when the
sequence changes. Stage is local; Save uses the existing expected-state,
backup, write, readback and typed recovery path. Unknown slots are read-only.
Failure/conflict keeps the core draft; failed reads also retain form text.
Rereading an unchanged slot retains unsubmitted event fields; a read that changes
the draft refreshes the form from the verified program. This is covered by a
headless memory-backend test, not an OS interaction test.
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

The repeat-count storage range and editable range are separate capabilities.
For Nia87, the wire can retain count zero while the counted-playback editor
admits only 1–65,535, matching the observed official editor policy. Loading
an existing zero does not rewrite the raw macro or invalidate its snapshot;
staging count zero, saving other edits with zero, and binding it are blocked.
The saved official HID replay has count one and no movement event, so this
policy does not establish repeat-zero or movement timing on firmware.

Recording uses a dedicated capture view, physical keyboard codes and five
pointer buttons. Measured timing uses host event timestamps in milliseconds;
fixed waits and a 50 ms measured tail preserve the established Nia87 policy.
For a backend with a smaller allowed maximum, the measured tail uses that
maximum. Existing events are not retimed. Core reserves releases and terminal
waits against the declared storage budget. Focus loss, Stop, input rejection
and close finish the recording locally; closing then checks whether to discard
the draft. Pointer motion/wheel and widget-consumed clicks are not recorded.

Macro file imports stage into the currently selected slot after validation;
source slot and binding metadata do not authorize a write or binding. Names
and compatible binding preferences survive document round trips. Incompatible
binding preferences are dropped with a notice. Exports create new files without
overwriting. Version-1 native macro files are accepted; exports use version 2.
See `macro-documents.md`. File activity is correlated and excludes edits/I/O;
failed imports retain the old draft, trust and metadata. Exporting an unverified
retained draft does not restore device trust.

Validation: 244 workspace library tests pass. New desktop tests run messages
through the memory executor, verify saved values by rereading, preserve zero
wait/count and signed motion, reject an out-of-range edit atomically, reject
opaque writes, and check close/failure/stale-result behavior. Form projections
round-trip every demo action kind. Windows and Linux all-target/all-feature
Clippy pass with warnings denied. Core checks for WASM; Windows release builds.
Recorder tests cover asymmetric waits, duplicate edges, old-prefix preservation,
held releases, exact capacity, oversized intervals, fixed timing, excluded UI
clicks, focus loss and close behavior. Physical-key mapping tests distinguish
keypad keys and left/right modifiers. These are headless input-routing tests;
OS event delivery and rendered capture interaction remain unverified.

File tests cover 64 KiB bounds, both versions, unchanged existing outputs,
validation before creating output, stale completion rejection, cross-backend
metadata, draft retention and close handling. Four legacy codec tests moved
from the root package into devices; they are not duplicated in the total.

This is not full acceptance. Iced stores local slot labels separately from
keyboard macro bytes; file names also travel as document metadata. No hardware
writes were performed for this UI step. The product CLI's read-only
`list-macros` command scanned the attached Nia87 and returned capacity 50,
configured `slot-00` only, and first free `slot-01`. The conservative
two-complete-copy USB scan took about 29 seconds on this host; it runs only
when the Macros page first needs a catalog or after invalidation. Rendered interaction,
physical playback, Linux runtime and power-cycle persistence remain unverified;
the screenshot helper failure documented in `iced-keymap-acceptance.md` remains
open. The earlier injected-failure recovery problem is also still open.
