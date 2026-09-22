# Configurator acceptance ledger

This ledger separates implemented configuration storage from end-to-end behavior. Source observations live in `feature-inventory.md`; attached-device evidence lives in `live-evidence.md`. Neither a shared OEM encoder nor an advertised catalog proves that every feature works on this board.

| Requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Native product without JS, Electron or IoT helper | Rust/egui application builds and launches; direct HID reads/writes work with OEM helper stopped | Target-specific dependency/license audit and distributable packaging |
| Linux configuration | Original hidraw adapter and full GUI cross-link to ELF64 using Zig | Native Linux GUI launch, hidraw permissions, hardware transactions |
| Plug-and-play Nia87 TKL | Attached device auto-loads built-in 87-key geometry and fixed slots | Stronger board/revision identification, reconnect and multiple-device behavior; clean-machine run |
| Base and Fn keymaps | Both maps read twice; base, Fn-only and mixed same-slot application writes/restores pass with one-second setter spacing | Broader action coverage, physical output and persistence |
| Modifier, media and mouse actions | Pure codecs and catalog, including 12 previously missing visible media/system actions with documented wire facts | Physical output and modifier-combination coverage |
| Macro storage | Long → short → empty replacement verified; stale-page clearing and transitional-read regression fixed; write-handle identity checked | Official replay comparison, other slots and boundary behavior |
| Macro editor | Native event table, focused recorder, repeats/modes, import/export and saved-slot binding; all three base and Fn binding modes read back and restored | Recorder GUI interaction validation, physical playback modes and timing |
| Global lighting | Native controls; selected cases for IDs0–19 pass write/read/restore; webcam confirms steady red/green | Broader visual/option coverage and reactive effects |
| Per-key colors | Native physical layout editor; single-color write/restore passes all 128 RGB values | GUI interaction coverage, picture-slot semantics and visual validation |
| Host-driven lighting | Captured actual0D music/0E screen commands; native Windows screen and WASAPI playback streams pass bounded hardware/camera/restoration checks; original frequency processing and GUI Start/Stop implemented | GUI interaction/close verification, sustained streaming, Linux audio, Wayland capture, Linux runtime, X11 disconnect handling |
| Other settings | Native debounce, auto-OS, four sleep timers and backlight switch pass reversible complete settings readback | Physical sleep behavior, remaining option semantics and report-rate support |
| Local configurations | Versioned raw-preserving keymap import/export with board/firmware/padding validation; macro JSON | Full settings/macro bundle semantics and GUI interaction coverage |
| Robust device transactions | Expected-state checks, disk backups, readback/rollback and OS-held interprocess lock; optional blocking Windows USB string reads removed | USB error recovery, disconnect/reconnect and partial failure tests |
| 2.4 GHz | Deferred by USB-first scope | Receiver identification, transport capability and separate verification when available |

Firmware flashing, vendor account login and cloud sharing are outside the local stock-firmware configurator scope. OEM database metadata and UI arrangements are not product implementation sources. Physical tests remain explicitly pending while the user is AFK.
