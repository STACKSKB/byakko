# Iced native archive capture and export

The public pre-alpha Diagnostic capture page captures the current device and
exports the captured native archive to a new file. It is a diagnostic tool;
archive import, review and restore/apply are not exposed in the UI. This
user-directed scope defers public archive restore because recorded failures
remain unresolved. It does not change the retained core/device restore APIs,
tests or research examples, which remain available to developers outside the
public workflow. Automatic per-feature before-image backups for ordinary
feature writes remain enabled.

The portable core stores bounded opaque bytes with a backend and format ID; it
has no paths or Nia87 section schema. The Nia87 archive contains both full
keymaps, all 50 raw macro slots, global lighting, all 128 picture triples and
four settings replies. Capture uses one complete sweep. The picture section
records only the response under the active lighting selector; selector context
is not a separate native picture payload.

The historical read-only capture at
`Research/captures/archive-phase1-20260923.json` used the former two-sweep
behavior, completed all 100 macro reads, and matched
`Research/captures/configuration-getter-trace-baseline.json` exactly: both
139,657 bytes, SHA-256
`2137480F0BA425BF06C9EF9A37881096834C928F6D4C21208ED65DAF210BA4D4`.
The retained CLI produced this file through the same then-current capture
function used by the Iced adapter. No setters were sent. This dated capture is
historical evidence; current capture behavior uses one sweep.

Core/device tests also cover import validation, two-direction restore
plans, summaries, and typed recovery through the memory backend. Those retained
developer APIs are not part of the public Iced workflow. A historical
fault-injected restore caused unexplained collateral changes, and a later
exact-restore run mismatched raw `FF FF FF` lighting bytes against `B4 B4 B4`
readback before verified rollback. The older Windows collateral change and the
raw-white mismatch remain unresolved. Deferring restore from the public UI
closes the release exposure gate by scope; it does not repair either failure or
establish restore acceptance. See the
[fault investigation](../Research/configuration-fault-verification.md) and
[Linux handoff](linux-handoff.md).

Linux current-source runtime and rendered capture/export interaction remain
unverified.

An interrupted export can leave a partial destination file; the UI reports its
path and requires a new destination rather than overwriting it.
