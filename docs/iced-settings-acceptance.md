# Iced scalar settings slice, 2026-09-23

The Iced desktop reads and edits scalar settings through the same portable
session and serialized executor used by other pages. Capabilities supply field
labels, toggle or numeric types, ranges, steps and units. The demo supplies
different fields to check that the desktop view does not depend on Nia87 IDs.
Core accepts one staged field at a time because the Nia87 native transaction
writes and verifies one setting. A second field can be staged after the first
is applied or reverted.

The Nia87 adapter advertises debounce, automatic OS mode, four sleep timers
and backlight. The visible sleep values are minutes; its native codec uses
seconds. A snapshot revision retains the four full 64-byte replies, including
unknown and reserved bytes. Noncanonical native values stay opaque. Forged
snapshots and invalid edits fail before device I/O. Native apply has an exact
expected-state check, durable backup, readback, rollback and typed recovery.

Opening Settings now reads an unloaded or invalidated snapshot once the
keymap is ready; a pending keymap read finishes first. Returning to a ready
page does not repeat it, and failed reads require an explicit retry. A
memory-backend test covers this page lifecycle.

The read-only capture in
`Research/captures/settings-core-executor-20260923.json` returned seven
editable values. Its 256 revision bytes match the settings section of
`Research/captures/configuration-getter-trace-baseline.json` exactly. No setter
was sent. A later independent CLI read returned the same seven editable
values and a repeat returned identical JSON (see `live-evidence.md`). Core,
memory and device tests cover range validation, one-field
staging, stale results, conflicts, readback mismatch, raw preservation and
recovery. Physical write/read/restore for this Iced path, rendered interaction
and Linux hardware behavior remain open acceptance work.
