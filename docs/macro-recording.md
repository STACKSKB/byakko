# Focused macro recording

Load a macro slot before recording. Start recording to append to the local
draft, then keep the capture pad focused. Recording does not write the keyboard;
review the event stream, save the macro, and bind a key separately.

The capture pad accepts keyboard transitions and five mouse buttons: left,
right, middle, back and forward. Mouse clicks outside the pad stop recording
instead of adding interactions with other controls. Stop and focus loss append
release events for held keys and buttons. Space for those releases is reserved
within the 248-byte encoded limit. Repeated down events are ignored.

This recorder is local to the native window; it does not hook global desktop
input. Pointer movement and wheel actions can be edited manually but are not
recorded automatically. **Wait after** is the interval from an event to the next
transition. Recording omits the initial wait before the first action, and does
not change earlier events when appending a recording. Stop/focus-loss releases
include the final held interval; time spent reaching Stop does not extend a
fully released recording. Intervals use frame timestamps rounded to
milliseconds; events within one frame share a timestamp. Keypad and left/right
modifier identity may be unavailable from the window event stream.

Headless event tests exercise the recorder and codec without sending input to
other applications or writing the device. They do not establish firmware
playback timing, movement semantics or physical key activation; those remain
hardware validation requirements.

The [timing audit](../Research/macro-recorder-timing.md) explains the correction
from the earlier recorder, which attached intervals one event late. Existing
macro files are not rewritten automatically.
New measured recordings finish with a 50 ms wait, matching the audited official
save path. Fixed delay uses the selected 1–65,535 ms value for recorded intervals
and the final wait. Synthetic releases have zero waits between them. Existing
manual/imported events, including explicit zero waits, remain unchanged.

Clear draft removes events locally while retaining repeat count and metadata.
Revert restores the loaded baseline; Save to keyboard applies the staged change.

Binding mode is stored on the key; repeat count is stored in the shared macro
slot. Toggle and hold modes require count 1 to match the official save policy.
If the slot has another count, **Stage count 1** changes the draft explicitly;
save it before binding. This count change affects every key using that slot.
Switching modes or importing a file never silently rewrites its repeat count.

## Implementation boundary

`macro_recorder` owns deterministic timing, held-input tracking, duplicate
suppression and encoded-capacity reservation. It receives timestamps explicitly
and has no egui, device, file or clock access. Rejected transitions leave the
draft and recorder state unchanged. Stop consumes the recording session and
releases held inputs in reverse order.

`MacroState` owns the slot, sole draft, paired raw/decoded baseline and explicit
read/apply activity. A failed operation retains its prior baseline and draft
while marking the baseline unverified. Matching readback restores trust; slot
changes and imports cannot replace a pending operation's draft. The editor lends
the draft to the recorder while recording and disables other edits during that
session. The UI translates physical input, handles capture focus and presents
recorder outcomes. Device worker dispatch and file/label controls remain in
`macro_ui`; the model performs no egui, file or HID operations.

A main-window device reread or full-archive apply invalidates the loaded panel
baselines, including macros. Drafts remain available for review/export, but
saving or binding requires a verified slot read. Reverting an invalidated draft
does not restore trust: reload the slot after reverting. A failed macro apply
also requires a new read before retrying.

## Local macro files

Import and export preserve the existing version-1 JSON format: slot, local
name, playback mode, repeat count and typed events. Imports are bounded to
64 KiB before JSON parsing and validate the 248-byte device encoding before
staging. Invalid files leave the current draft unchanged. Exports validate the
entire output before creating a new file and never overwrite an existing one.
Importing into a loaded slot stages data there; the file's slot is metadata,
not permission to write another device slot. Neither file operation writes HID.
