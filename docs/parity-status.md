# Configurator acceptance ledger

This ledger separates implemented configuration storage from end-to-end behavior. Source observations live in `feature-inventory.md`; attached-device evidence lives in `live-evidence.md`. Neither a shared OEM encoder nor an advertised catalog proves that every feature works on this board.

| Requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Native product without JS, Electron or IoT helper | Rust/egui application builds and launches; direct HID reads/writes work with OEM helper stopped | Target-specific dependency/license audit and distributable packaging |
| Linux configuration | Linux HID backend selected in Cargo | Actual Linux build, native GUI launch, hidraw permissions, hardware transactions |
| Plug-and-play Nia87 TKL | Attached device auto-loads built-in 87-key geometry and fixed slots | Stronger board/revision identification, reconnect and multiple-device behavior; clean-machine run |
| Base and Fn keymaps | Both maps read twice; base remap/restore passes complete readback | Fn write/restore, action coverage, physical output and power-cycle persistence |
| Modifier, media and mouse actions | Pure codecs and catalogs implemented | Compare the Nia87-visible official action set, verify missing actions and physical output |
| Macro storage | Long → short → empty replacement verified; stale-page regression fixed | Official replay comparison, other slots and boundary behavior |
| Macro editor | Native event table, repeats/modes, import/export and saved-slot binding implemented | GUI interaction validation, recording workflow, binding/playback mode tests and physical timing |
| Global lighting | Catalog codec and repeated live read pass | Backed-up write/restore, each applicable parameter family, native controls, visual validation |
| Per-key colors | Pure slot-indexed codec and six-page read parser | Live picture read/write/restore, native color editing, picture-slot semantics |
| Host-driven lighting | Official catalog advertises screen and music modes | Determine actual required host processing; independently implement applicable behavior |
| Other settings | Nia87 advertises debounce and Bluetooth/2.4 GHz sleep ranges | Trace selected commands, capture/read/write/restore and native controls; resolve report-rate support |
| Local configurations | Keymap backups and macro JSON exist | Complete local profile import/export and restore semantics compared with official local workflows |
| Robust device transactions | Expected-state checks, disk backups and readback/rollback | Serialize concurrent app instances; disconnect/reconnect and partial failure tests |
| 2.4 GHz | Deferred by USB-first scope | Receiver identification, transport capability and separate verification when available |

Firmware flashing, vendor account login and cloud sharing are outside the local stock-firmware configurator scope. OEM database metadata and UI arrangements are not product implementation sources. Physical tests remain explicitly pending while the user is AFK.
