# Iced USB discovery acceptance

The Nia87 composition root maps its read-only HID configuration-collection
enumeration to a backend-neutral desktop availability state. The discovery
worker has one outstanding scan and never opens a device or sends a feature
report. At startup Iced scans, then reads a unique match through the existing
serialized device executor. Idle scans notice removal and retry a read after
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
machine startup still need acceptance. Native write transactions currently
re-enumerate rather than pinning a selected configuration path; candidate
pinning is a remaining safety gate before treating hotplug as fully verified.
