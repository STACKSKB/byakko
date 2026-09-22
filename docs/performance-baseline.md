# Windows release process baseline

Measured on the development Windows host on 2026-09-22, at source commit
`a0b83f9`, using `cargo build --release --locked --bin byakko` with default
features and Cargo's standard release profile. No vendor helper or other Byakko
process was running at launch. The native window opened with the keyboard
attached; no configuration edits or lighting streams were started.

| Measurement | Result |
| --- | ---: |
| Executable | 9,043,456 bytes (8.62 MiB) |
| Observation interval | 75.93 seconds |
| Process CPU time during interval | 0.50 seconds |
| CPU relative to one logical core | 0.66% |
| Final private committed memory | 109,133,824 bytes (104.08 MiB) |
| Final working set | 123,686,912 bytes (117.96 MiB) |
| Final thread count | 9 |

Executable SHA-256:
`b7ba857c0d938579256c5f59fdae1a0da2ac184f47af2a4a016fbcff5a560efb`.
Samples are retained locally in ignored files
`Research/captures/release-idle-start.json` and
`Research/captures/release-idle-end-corrected.json`. The earlier
`release-idle-end.json` has an invalid elapsed-time calculation and is not used.

This is an initial near-idle observation, not a controlled benchmark. It includes
startup settling and accessibility/screenshot queries. Process CPU is summed
across threads; private bytes are committed memory, while working set includes
resident shared pages. GPU allocation and system-wide graphics costs are not
measured. No comparable Synapse/iCUE measurements were collected.

The computer-use screenshot helper failed twice with
`SetIsBorderRequired: No such interface supported (0x80004002)`. Accessibility
exposed the title bar only, so this run does not prove loaded-panel content or
visual correctness. Alt-F4 closed the app normally; a subsequent process query
confirmed no Byakko process remained.

Source review found no unconditional idle repaint loop. Device work requests
repaints at 100 ms intervals; read retries have bounded scheduling. Per-frame
layout and change-list allocations exist, but do not themselves schedule frames.
No speculative performance changes were made from this single sample. Sustained
lighting, active editing, Linux runtime, and repeatable foreground/background
measurements remain open.
