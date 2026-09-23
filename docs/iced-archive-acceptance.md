# Iced native archive capture and review, 2026-09-23

The local configurations page captures the current device, exports the
captured native archive to a new file, and reviews a chosen archive against a
fresh complete capture. The portable core stores bounded opaque bytes with a
backend and format ID; it has no paths or Nia87 section schema. The backend
validates the native JSON, checks both forward and reverse restoration plans,
and returns named section counts. Imported bytes must fit the advertised size
limit. A write or connection change invalidates a review.

The Nia87 archive contains both full keymaps, all 50 raw macro slots, global
lighting, all 128 picture triples and four settings replies. Its existing
capture makes two complete sweeps and requires them to match. The read-only
capture at `Research/captures/archive-phase1-20260923.json` completed all
100 macro reads and matched
`Research/captures/configuration-getter-trace-baseline.json` exactly: both
139,657 bytes, SHA-256
`2137480F0BA425BF06C9EF9A37881096834C928F6D4C21208ED65DAF210BA4D4`.
The retained CLI produced this file through the same native capture function
used by the Iced adapter. No setters were sent.

Core/device/desktop tests cover size and identity bounds, stale completions,
exact target echo, invalidation, malformed imports, two-direction plans,
section summaries and file-task close failure. The Iced page does not offer
archive apply yet. The existing native apply reports recovery in error text;
it needs a typed outcome before this workflow can safely surface it. The
earlier injected failure and subsequent explicit restore remain a separate
physical acceptance gate. Linux hardware and rendered interaction are also
unverified for this slice.
An interrupted export can leave a partial destination file; the UI reports
its path and requires a new destination rather than overwriting it.
