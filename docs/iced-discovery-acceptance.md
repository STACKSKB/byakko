# Iced USB discovery acceptance

The Nia87 composition root maps its read-only HID configuration-collection
enumeration to a backend-neutral desktop availability state. The discovery
worker has one outstanding scan and never opens a device or sends a feature
report. At startup Iced attaches a serialized executor to a single matching
collection. A wired collection and a receiver together require an explicit
transport choice. Every later device open checks
the selected path and HID collection metadata, including setter-handle reopens
and archive recovery readback. Idle scans notice removal and retry a read after
reconnection to the pinned endpoint. Same-transport duplicates and enumeration
errors prevent selection. **Find connections** requests another scan while a
healthy notification listener is active.

The session preserves staged drafts on disconnect. On reconnection a complete
read verifies the baseline or reports a conflict; it never applies a draft
automatically. An uncertain write or conflict suspends automatic rereads until
the user explicitly requests one. Scan tickets and connection generations reject
late results, and the desktop drains late executor completions after disconnect.

Headless tests cover no device, one or multiple matches, enumeration error,
draft retention, stale completion rejection and failed-write behavior. Physical
read-only enumeration on the attached Windows machine returned exactly one
`3151:4015` configuration collection at interface 2 and usage `FFFF:0002`.
It sent no HID reports. Physical unplug/replug, Linux runtime permissions,
multiple-device identity and clean
machine startup still need acceptance. The pin includes the OS-reported USB
parent when available. A physically identical replacement at the same path
and parent, or a receiver re-paired to another Nia87, can present identical
metadata. Start a fresh session when deliberately making either change.
Dual-transport selection and writes still need physical verification.
