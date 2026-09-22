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
include the final held interval; a fully released recording has no added tail
for time spent reaching Stop. Intervals use frame timestamps rounded to
milliseconds; events within one frame share a timestamp. Keypad and left/right
modifier identity may be unavailable from the window event stream.

Headless event tests exercise the recorder and codec without sending input to
other applications or writing the device. They do not establish firmware
playback timing, movement semantics or physical key activation; those remain
hardware validation requirements.

The [timing audit](../Research/macro-recorder-timing.md) explains the correction
from the earlier recorder, which attached intervals one event late. Existing
macro files are not rewritten automatically.
The follow-up audit also identifies an outstanding save-time difference: the
official timeline adds a final50ms delay when it ends in an action (or its fixed
recording delay when enabled). Native recorded events currently end with zero
wait. This remains a timing-parity gap, especially for repeated playback.

## Implementation boundary

`macro_recorder` owns deterministic timing, held-input tracking, duplicate
suppression and encoded-capacity reservation. It receives timestamps explicitly
and has no egui, device, file or clock access. Rejected transitions leave the
draft and recorder state unchanged. Stop consumes the recording session and
releases held inputs in reverse order.

The editor owns the draft and lends it to the recorder while recording; other
draft edits are disabled during that session. The UI translates physical input,
handles capture focus and presents typed recorder outcomes. Macro device workers
and file/label controls still live in `macro_ui` and remain separate cleanup work.

## Local macro files

Import and export preserve the existing version-1 JSON format: slot, local
name, playback mode, repeat count and typed events. Imports are bounded to
64 KiB before JSON parsing and validate the 248-byte device encoding before
staging. Invalid files leave the current draft unchanged. Exports validate the
entire output before creating a new file and never overwrite an existing one.
Importing into a loaded slot stages data there; the file's slot is metadata,
not permission to write another device slot. Neither file operation writes HID.
