# Iced native archive capture and review

The local configurations page captures the current device, exports the
captured native archive to a new file, and reviews a chosen archive against a
fresh complete capture. The portable core stores bounded opaque bytes with a
backend and format ID; it has no paths or Nia87 section schema. The backend
validates the native JSON, checks both forward and reverse restoration plans,
and returns named section counts. Imported bytes must fit the advertised size
limit. A write or connection change invalidates a review.

The Nia87 archive contains both full keymaps, all 50 raw macro slots, global
lighting, all 128 picture triples and four settings replies. Capture uses one
complete sweep. The read-only picture section records only the picture response
under the active lighting selector; selector context is not a separate native
picture payload.
Preflight rejects a restore that both changes this selector and writes picture
colors; the target option can be selected and captured separately first.
The historical read-only capture at
`Research/captures/archive-phase1-20260923.json` used the former two-sweep
behavior, completed all 100 macro reads, and matched
`Research/captures/configuration-getter-trace-baseline.json` exactly: both
139,657 bytes, SHA-256
`2137480F0BA425BF06C9EF9A37881096834C928F6D4C21208ED65DAF210BA4D4`.
The retained CLI produced this file through the same then-current capture
function used by the Iced adapter. No setters were sent. This dated capture is
retained as historical evidence; current capture behavior uses one sweep.

Core/device/desktop tests cover size and identity bounds, stale completions,
exact target echo, invalidation, malformed imports, two-direction plans,
section summaries, file-task close failure and guarded apply through the
memory backend. The Iced page offers apply only for a changed review. Native
apply uses the reviewed cached before-image, saves a durable backup, writes
supported changes and captures one complete readback. It does not repeat a
pre-write sweep. Its structured result
distinguishes pre-write rejection (`NotAttempted`), verified restoration,
definite restoration mismatch and unreadable restoration. Failed apply keeps
the review visible but requires fresh review before another attempt. The
earlier injected failure and subsequent explicit restore remain a separate
physical acceptance gate; the failed automatic recovery discrepancy is still
unresolved. No live Iced archive write has been accepted. Linux current-source
runtime and rendered archive interaction remain unverified.
An interrupted export can leave a partial destination file; the UI reports
its path and requires a new destination rather than overwriting it.
