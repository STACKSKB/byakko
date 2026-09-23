# Iced per-key picture slice, 2026-09-23

The Iced desktop now exposes per-key RGB picture storage through the core
picture capability and serialized device executor. The backend supplies the
key IDs; Nia87 maps its physical keys to matrix slots. Its native revision
retains all 128 RGB triples, including reserved and unmapped slots, while the
editable catalog covers mapped physical keys including Fn. Reserved slots
remain unchanged. Direct picture writes and native archive review now also
reject changes to unmapped matrix slots before any setter; unchanged
unmapped values remain in backups. Opaque or forged snapshots and stale
baselines are rejected before a write is attempted. Device apply uses the
existing backup, readback and restore transaction and reports recovery with a
typed outcome.

This RGB storage feature is separate from the global lighting control that
selects a built-in picture effect. Host-driven effects use a separate stream
lifecycle and still need physical Iced acceptance. 2.4 GHz configuration remains deferred
until USB support and receiver capability are established.

Opening the per-key color page automatically reads the device when its color
snapshot is unloaded or invalidated and the keymap is ready. If the keymap is
still reading, it starts after that completion. Page revisits do not repeat a
verified read, and failed or uncertain reads require an explicit retry.
A memory-backend reconnect test confirms that an automatic keymap and color
refresh preserves a staged per-key draft.

The memory backend and device adapter tests cover capability validation,
baseline conflicts, opaque state, executor dispatch, Fn and final-key mapping,
full 128-slot round trips, reserved-slot preservation, forged content and
typed recovery. The attached Nia87 was verified read-only for this Iced slice.
The independent CLI later read all 87 mapped colors through the same session
and executor; see `live-evidence.md` for its capture and hash.
The 384-byte result in `Research/captures/picture-core-executor-20260923.json`
matches `Research/captures/picture-initial.json` exactly. It differs from
`Research/captures/picture-after-failed-recovery.json` at slot 9 red (current
0, earlier 255); the cause of that intervening change is unknown.
No live picture writes were performed, so physical color output, write/read/
restore behavior, visual accuracy and Linux hardware behavior remain open
acceptance work. Headless tests do not establish those behaviors.
