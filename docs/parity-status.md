# Configurator acceptance ledger

This ledger separates implemented configuration storage from end-to-end behavior. Source observations live in `feature-inventory.md`; attached-device evidence lives in `live-evidence.md`. Neither a shared OEM encoder nor an advertised catalog proves that every feature works on this board.

The table records the retained research application's feature evidence. The
approved Iced migration currently has keymap, macro, built-in global lighting
per-key picture and scalar settings editing/read/save slices, plus native archive
capture/export/review/apply;
it has not yet inherited the remaining screens or feature parity. See
`iced-keymap-acceptance.md`, `iced-macro-acceptance.md`,
`iced-lighting-acceptance.md`, `iced-picture-acceptance.md` and
`iced-settings-acceptance.md` and `iced-archive-acceptance.md` for their exact
scope.

| Requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Native product without JS, Electron or IoT helper | Rust/egui application builds and launches; direct HID reads/writes work with OEM helper stopped | Target-specific dependency/license audit and distributable packaging |
| Reusable frontend | Shared keymap editor with dynamic layout/layers, typed actions and Nia87 adapter; device-free 12-key/three-layer backend tests | Migrate discovery and remaining panels; implement and validate independent backends such as QMK/VIA |
| Linux configuration | Original hidraw adapter and full GUI cross-link to ELF64 using Zig | Native Linux GUI launch, hidraw permissions, hardware transactions |
| Plug-and-play Nia87 TKL | Attached device auto-loads built-in 87-key geometry and fixed slots. Iced now scans the USB configuration collection without sending HID reports, starts a read on one match, notices idle removal, and re-reads after reconnection. Multiple matches and enumeration errors are explicit; headless reconnect tests retain staged drafts and reject stale results. | Physical unplug/replug and clean-machine acceptance; stronger board/revision identification and candidate pinning across transactions; cross-panel reconnect validation |
| Base and Fn keymaps | Both maps read twice; base, Fn-only and mixed same-slot application writes/restores pass with one-second setter spacing | Broader action coverage, physical output and persistence |
| Modifier, media and mouse actions | Pure codecs and catalog, including 12 previously missing visible media/system actions with documented wire facts | Physical output and modifier-combination coverage |
| Macro storage | Exact 248-byte → short → empty replacement verified in slots1/24/49; 33 mixed key/mouse/movement events and delay boundaries stored exactly in slot49, then full configuration restoration verified; official short-macro capture confirms 56-byte page header | Broader official replay vectors, physical playback and timing; repeat-zero and movement-delay semantics |
| Macro editor | Native event table, focused keyboard/mouse-button recorder, repeats/modes, local labels, import/export and saved-slot binding; base/Fn binding modes read back and restore; recorder intervals attach to the preceding action; fixed-delay recording, reversible Clear draft and terminal-delay policy covered by pure/headless tests; toggle/hold require explicitly staged count 1 before save/bind | OS GUI interaction validation; physical playback modes and timing |
| Global lighting | Native controls; selected cases for IDs0–19 pass write/read/restore; webcam confirms steady red/green | Broader visual/option coverage and reactive effects |
| Per-key colors | Retained research GUI has native physical layout editor; earlier single-color write/restore passed, while selected official bulk reader/writer addresses index0 and has three separate global effect options (see `Research/picture-selector-audit.md`). Iced now has a capability-driven RGB editor and memory workflow; the attached USB baseline was checked read-only, with no live writes in this slice. RGB storage is separate from selecting the global picture effect. | Iced GUI interaction and visual validation; physical per-key write/read/restore acceptance; effect-option semantics |
| Host-driven lighting | Captured actual0D music/0E screen commands; Windows screen/WASAPI streams pass hardware/camera/restoration checks; Linux output-monitor sampler cross-builds; headless close-event success/failure checks pass | OS GUI interaction/close verification, sustained streaming, Wayland capture, Linux runtime/audio routing, X11 disconnect handling |
| Other settings | Native debounce, auto-OS, four sleep timers and backlight switch pass reversible complete settings readback. Iced now exposes these seven fields through capability data with one-field staged saves; its read-only 256-byte USB capture matches the earlier complete archive exactly. | Iced physical write/read/restore and rendered UI acceptance; physical sleep behavior and remaining option semantics; report-rate metadata exists but the selected official Nia87 descriptor does not enable its control (see `Research/report-rate-selected-path.md`) |
| Local configurations | Retained research app has full native archive capture, review and guarded apply; normal multi-section round trip passed, while first injected failure failed automatic recovery and an explicit later restore succeeded. Iced now captures, exports, reviews with forward/reverse preflight and offers guarded apply with typed recovery; a read-only complete capture matched the earlier baseline byte for byte. No live Iced archive write has been attempted. | Resolve unexplained changes during failed recovery; repeat physical failure-injection acceptance for the typed path and validate OS GUI interaction |
| Robust device transactions | Expected-state checks, disk backups, readback/rollback and OS-held interprocess lock; optional blocking Windows USB string reads removed | USB error recovery, disconnect/reconnect and partial failure tests |
| 2.4 GHz | Deferred by USB-first scope | Receiver identification, transport capability and separate verification when available |

Host-driven effects remain a later capability; they require a distinct host
stream lifecycle and are not part of per-key RGB picture storage.

Firmware flashing, vendor account login and cloud sharing are outside the local stock-firmware configurator scope. OEM database metadata and UI arrangements are not product implementation sources. Physical tests remain explicitly pending while the user is AFK.
