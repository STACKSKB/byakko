# Iced scalar settings slice

The Iced desktop reads and edits scalar settings through the same portable
session and serialized executor used by other pages. Capabilities supply field
labels, toggle or numeric types, ranges, steps and units. The demo supplies
different fields to check that the desktop view does not depend on Nia87 IDs.
Core stages one field per transaction because the Nia87 native transaction
writes and verifies one setting. The desktop queue can retain pending edits for
multiple fields and serializes them as separate one-field transactions.

The Nia87 adapter advertises debounce, automatic OS mode, four sleep timers
and backlight. The visible sleep values are minutes; its native codec uses
seconds. A snapshot revision retains the four full 64-byte replies, including
unknown and reserved bytes. Noncanonical native values stay opaque. Forged
snapshots and invalid edits fail before device I/O. A settings write backs up
the cached before-image, writes one field, and performs one complete settings
readback. The ordinary Iced apply path does not issue a repeated pre-write
read. Failure and recovery outcomes remain explicit.

Connection refresh loads the scalar sections once. Navigation to Settings does
not issue reads; failed or invalidated snapshots require a deliberate read.
A memory-backend test covers the connection/page lifecycle.

The read-only capture in
`Research/captures/settings-core-executor-20260923.json` returned seven
editable values. Its 256 revision bytes match the settings section of
`Research/captures/configuration-getter-trace-baseline.json` exactly. No setter
was sent. A later independent CLI read returned the same seven editable
values and a repeat returned identical JSON (see `live-evidence.md`). The
official-app capture from 2026-09-25 observed six settings setters without an
immediate settings GET; it is bounded reference evidence, not a reason to
remove Byakko's verification. Iced loads scalar sections once on connection
refresh; page navigation does not read again. The current Iced physical
settings write/read/restore cycle remains unaccepted. Core, memory and device
evidence covers one-field staging, stale results, conflicts, readback mismatch,
raw preservation and recovery. Linux runtime acceptance is tracked separately
in `linux-handoff.md`.
