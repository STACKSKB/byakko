# Configurator acceptance ledger

This ledger separates implemented configuration storage from end-to-end behavior. Source observations live in `feature-inventory.md`; attached-device evidence lives in `live-evidence.md`. Neither a shared OEM encoder nor an advertised catalog proves that every feature works on this board.

| Requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Native product without JS, Electron or IoT helper | Rust/egui application builds and launches; direct HID reads/writes work with OEM helper stopped | Target-specific dependency/license audit and distributable packaging |
| Linux configuration | Original hidraw adapter and full GUI pass Linux cross-target checks | Linux link/build, native GUI launch, hidraw permissions, hardware transactions |
| Plug-and-play Nia87 TKL | Attached device auto-loads built-in 87-key geometry and fixed slots | Stronger board/revision identification, reconnect and multiple-device behavior; clean-machine run |
| Base and Fn keymaps | Both maps read twice; base remap/restore and captured native Fn media replay pass complete readback | Fn-only application transactions pass after USB recovery; mixed same-slot edits are guarded; broader action coverage, physical output and persistence remain |
| Modifier, media and mouse actions | Pure codecs and catalog, including 12 previously missing visible media/system actions with documented wire facts | Physical output and modifier-combination coverage |
| Macro storage | Long → short → empty replacement verified; stale-page clearing and transitional-read regression fixed; write-handle identity checked | Official replay comparison, other slots and boundary behavior |
| Macro editor | Native event table, focused recorder, repeats/modes, import/export and saved-slot binding; all three base and Fn binding modes read back and restored | Recorder GUI interaction validation, physical playback modes and timing |
| Global lighting | Native controls; brightness write/read/restore passes full settings comparison | Each applicable parameter family and visual validation |
| Per-key colors | Native physical layout editor; single-color write/restore passes all 128 RGB values | GUI interaction coverage, picture-slot semantics and visual validation |
| Host-driven lighting | Official catalog advertises screen and music modes | Determine actual required host processing; independently implement applicable behavior |
| Other settings | Native debounce, auto-OS and four sleep timers; all pass reversible complete settings readback; options read-only | Physical sleep behavior, option semantics and report-rate support |
| Local configurations | Versioned raw-preserving keymap import/export with board/firmware/padding validation; macro JSON | Full settings/macro bundle semantics and GUI interaction coverage |
| Robust device transactions | Expected-state checks, disk backups, readback/rollback and OS-held interprocess lock; optional blocking Windows USB string reads removed | USB error recovery, disconnect/reconnect and partial failure tests |
| 2.4 GHz | Deferred by USB-first scope | Receiver identification, transport capability and separate verification when available |

Firmware flashing, vendor account login and cloud sharing are outside the local stock-firmware configurator scope. OEM database metadata and UI arrangements are not product implementation sources. Physical tests remain explicitly pending while the user is AFK.
