# Byakko v0.1.0-pre-alpha.1

**Experimental pre-alpha.** Native Windows/Linux configuration for the stock
USB Menel Nia87. This release replaces the previous application orchestration
with shared domain editors, a serialized native executor and equivalent Iced
views. It does not flash firmware.

Includes base/Fn key assignments, locally named recorded macros, onboard lighting,
per-key RGB layers 1/2/3 (Fn+Z/X/C), screen/music lighting, keyboard settings and
diagnostic exports. Native onboard-change events replace periodic feature polling.
Fn system-key positions and Fn+Esc are protected from reassignment.

Portable packages contain the desktop and CLI, GPL license and dependency notices.
Linux also includes the permission helper, udev rule and desktop entry. Linux users
must follow `docs/linux-install.md`; run the application as the normal desktop
user. Screen capture requires X11; music capture requires PulseAudio or compatible
PipeWire services. Wayland screen capture is unsupported.

Software tests and rendered reviews do not prove every physical result. Independent
RGB layer 2/3 persistence, some macro playback cases, host restoration and Linux
hardware/runtime behavior still need acceptance checks. Linux onboard-event
permissions require validation; manual reads remain available. Historical recovery
failures remain unresolved, and full diagnostic archive restoration is not a public
UI feature. See the repository's dated acceptance and research records for details.

The UI remains English. Wireless receivers, other keyboards and QMK/VIA are outside
this release. Include the package version, OS and reproduction steps in bug reports.
