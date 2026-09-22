# Configurator acceptance ledger

This ledger separates implemented configuration storage from end-to-end behavior. Source observations live in `feature-inventory.md`; attached-device evidence lives in `live-evidence.md`. Neither a shared OEM encoder nor an advertised catalog proves that every feature works on this board.

| Requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Native product without JS, Electron or IoT helper | Rust/egui application builds and launches; direct HID reads/writes work with OEM helper stopped | Target-specific dependency/license audit and distributable packaging |
| Linux configuration | Original hidraw adapter and full GUI cross-link to ELF64 using Zig | Native Linux GUI launch, hidraw permissions, hardware transactions |
| Plug-and-play Nia87 TKL | Attached device auto-loads built-in 87-key geometry and fixed slots | Stronger board/revision identification, reconnect and multiple-device behavior; clean-machine run |
| Base and Fn keymaps | Both maps read twice; base, Fn-only and mixed same-slot application writes/restores pass with one-second setter spacing | Broader action coverage, physical output and persistence |
| Modifier, media and mouse actions | Pure codecs and catalog, including 12 previously missing visible media/system actions with documented wire facts | Physical output and modifier-combination coverage |
| Macro storage | Exact 248-byte → short → empty replacement verified in slots1/24/49; complete readback/restoration; official short-macro capture confirms 56-byte page header; stale-page clearing and transitional-read regression fixed | Broader official replay vectors, physical playback and timing; repeat-zero and movement-delay semantics |
| Macro editor | Native event table, focused recorder, repeats/modes, import/export and saved-slot binding; base/Fn binding modes read back and restore; headless egui events verify physical-key selection, repeat suppression and focus-loss releases | OS GUI interaction validation, physical playback modes and timing |
| Global lighting | Native controls; selected cases for IDs0–19 pass write/read/restore; webcam confirms steady red/green | Broader visual/option coverage and reactive effects |
| Per-key colors | Native physical layout editor; single-color write/restore passes all 128 RGB values | GUI interaction coverage, picture-slot semantics and visual validation |
| Host-driven lighting | Captured actual0D music/0E screen commands; Windows screen/WASAPI streams pass hardware/camera/restoration checks; Linux output-monitor sampler cross-builds; headless close-event success/failure checks pass | OS GUI interaction/close verification, sustained streaming, Wayland capture, Linux runtime/audio routing, X11 disconnect handling |
| Other settings | Native debounce, auto-OS, four sleep timers and backlight switch pass reversible complete settings readback | Physical sleep behavior, remaining option semantics and report-rate support |
| Local configurations | Full native archive capture, review and guarded apply; multi-section keymap/macro/picture/brightness/debounce apply and original restoration pass complete hardware readback; pure preflight rejects unsupported changes | Failure-injection recovery acceptance and GUI interaction coverage; independent picture-bank identification |
| Robust device transactions | Expected-state checks, disk backups, readback/rollback and OS-held interprocess lock; optional blocking Windows USB string reads removed | USB error recovery, disconnect/reconnect and partial failure tests |
| 2.4 GHz | Deferred by USB-first scope | Receiver identification, transport capability and separate verification when available |

Firmware flashing, vendor account login and cloud sharing are outside the local stock-firmware configurator scope. OEM database metadata and UI arrangements are not product implementation sources. Physical tests remain explicitly pending while the user is AFK.
