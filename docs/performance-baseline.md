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

## Initial Iced keymap slice

2026-09-22, built with `cargo build -p byakko-desktop --release --locked` using
Iced 0.14, tiny-skia, system fonts and Cargo's standard release profile.
Normal startup with the attached keyboard; no edits, Apply or streaming.
The window opened and later closed normally with Alt-F4. The screenshot helper
again failed twice with `SetIsBorderRequired` / `0x80004002`; the accessibility
tree exposed only the title bar. This does **not** verify that the device read
succeeded, the visual layout is correct or mouse interactions work.

| Measurement | Result |
| --- | ---: |
| Executable | 5,053,952 bytes (4.82 MiB) |
| Observation interval | 51.365 seconds |
| Process CPU time during interval | 0.765625 seconds |
| CPU relative to one logical core | 1.49% |
| Final private committed memory | 10,117,120 bytes (9.65 MiB) |
| Final working set | 27,000,832 bytes (25.75 MiB) |
| Final thread count | 7 |

Executable SHA-256:
`8ed9d154e75f6bdf1852410bd586fe8dcfd25d4eb9a5a04448b3ab42df2999f3`.
Samples: ignored `Research/captures/iced-idle-start.json` and
`Research/captures/iced-idle-end.json`. The interval includes helper queries and
is not a controlled benchmark. The Iced slice has much less functionality than
the legacy GUI; these measurements do not establish a toolkit performance win.
No unconditional rendering feature is enabled. Completion polling is subscribed
only while the core is Loading/Applying; rendered status was not observable in
this run, so a verified loaded-idle benchmark remains outstanding.

## Attached Nia87: Byakko and Sharkfin warmed idle

Measured on the same Windows host on 2026-09-23 from 13:18:51 to 13:19:07
IST. The Nia87 was attached over USB. The current Byakko Iced release window
had been open since 10:13; the installed Sharkfin window had been open since
13:17 and identified the board as “Menel Nia 87”. No editing, Apply action, or
lighting stream was started during the sample. Both windows were open during
measurement; Sharkfin was closed normally afterward. The Byakko executable
was built earlier that day, before the CLI-only changes at `878b4e8`.

| Measurement | Byakko desktop | Sharkfin plus six WebView2 children |
| --- | ---: | ---: |
| Executable size | 6,650,880 bytes | 13,885,440 bytes (host executable only) |
| Mean working-set sum, 16 one-second samples | 25.6 MiB | 426.7 MiB |
| Maximum working-set sum | 25.6 MiB | 430.5 MiB |
| Mean private committed-memory sum | 10.6 MiB | 225.5 MiB |
| Process CPU time gained over 15.58 seconds | 0.02 seconds | 0.58 seconds |

The Sharkfin process family was identified from Windows parent process IDs:
`sharkfin.exe` → one browser `msedgewebview2.exe` → five further WebView2
children. Other WebView2 processes on the machine had a different parent and
were excluded. `Get-Process` supplied per-process working set, private memory,
and cumulative CPU time at each sample. The table sums the process family; it
does not measure GPU memory, system-wide graphics allocation, or energy.
Working-set sums double-count pages shared between those processes, so private
committed memory is the more useful cross-process comparison here. The CPU
interval is short and nearly idle, so its relative percentages are noisy.

SHA-256 of the measured Byakko binary:
`ee8091a99b7d10e26075e0e7cda8d01d5d0ad156fbcf4b03236e7debc9345df8`.
SHA-256 of the installed Sharkfin binary:
`ff0b8e19abb7533c1bec1c9f4ecabaa8a1702a7d4c0c591eaa9d64d7e291e733`.

This establishes a same-host idle footprint, not a full performance ranking.
The apps were on different pages and have different completed feature sets.
Repeat with matched keymap, macro, per-key color, and host-stream workloads,
including foreground interaction and sustained use. The official app was not
running in this sample. The screenshot capture helper still returned
`SetIsBorderRequired`/`0x80004002`, so Byakko's page was not visually checked
by this measurement; its open window was observable through the title bar.
