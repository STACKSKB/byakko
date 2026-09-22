# Host-lighting close lifecycle

2026-09-22. A review found that polling a completed stream before handling a window-close event could lose a restoration error arriving in the same UI frame: the stop handle had already been cleared, so the close was no longer cancelled.

The workbench now handles close requests before polling workers and again after the UI has processed completion. A running stream cancels close and requests stop. Success permits the deferred close; restoration failure leaves the error visible and clears the closing intent. Stream worker panics are converted into completion errors, preventing the interface from remaining permanently busy. The error explicitly leaves restoration unverified; no claim is made that a panic always restores hardware state.

Three headless egui event tests pass: pending close waits, successful completion permits close, and same-frame restoration failure cancels close. A panic test confirms a completion error is produced. The success/wait cases share one test, for three tests total. The headless harness explicitly consumes/discards font texture deltas; it does not substitute for an OS-window integration test.

No device writes were needed for this lifecycle regression. Native window Start/Stop and close testing remains pending because the available capture tool fails with Windows interface error0x80004002.

A separate headless egui macro-recorder test now sends modifier changes and physical/logical key events through `process_recording`, then simulates loss of window focus. It verifies physical A is recorded despite a logical Z label, repeat keydowns are omitted, held A and Control are released, and the resulting macro still encodes. This tests the actual recorder event path without synthesizing keyboard input to other applications or claiming firmware playback.

## Ordinary configuration workers

The same close-before-completion ordering now covers ordinary global lighting,
per-key colors, settings and the legacy keymap path used to bind saved macros.
These operations cancel close while running, retain staged data on error, and
leave verification/recovery errors visible. They do not close automatically
when a worker finishes; the user can close again after reviewing the result.
Host lighting retains its separate stop-and-restore deferred-close behavior.

Device worker panics become completion errors that explicitly leave device
state/restoration unverified. This prevents a permanently busy UI but cannot
promise rollback after arbitrary panics. Headless close/error tests exercise
these panels without HID writes; direct panic tests cover the lighting and
macro wrappers. OS-window close testing and hardware
fault-recovery acceptance remain separate pending gates.
