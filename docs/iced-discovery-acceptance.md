# Iced USB discovery acceptance

The Nia87 composition root maps its read-only HID configuration-collection
enumeration to a backend-neutral desktop availability state. The discovery
worker has one outstanding scan and never opens a device or sends a feature
report. At startup Iced scans, then attaches a serialized executor with an
immutable native target for the unique match. Every later device open checks
the selected path and HID collection metadata, including setter-handle reopens
and archive recovery readback. Idle scans notice removal and retry a read after
reconnection. Ambiguous matches and enumeration errors do not choose a device.

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
machine startup still need acceptance. An OS path reused by another physically
identical board cannot be distinguished by this HID metadata alone, so native
writes still require complete expected-state checks. The new bound path has
not had a live Iced write acceptance run.
