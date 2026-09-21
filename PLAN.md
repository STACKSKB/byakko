# Byakko: plug-and-play Menel Nia87 native configurator

Research date: 2026-09-21; updated 2026-09-22. Scope: USB configuration using stock firmware only; 2.4 GHz is a later compatibility target. Native keymap and short macro storage have passed reversible write/readback checks on the connected keyboard. See `docs/live-evidence.md` for evidence and remaining verification limits. No firmware flashing is in scope.

## Direction

Use an original dense, keyboard-friendly native workbench. The user's references are McMaster-Carr, FL Studio, and Vim; do not copy the official GUI or Sharkfin UX. See `docs/design-direction.md` for interaction and review criteria.

Build a Linux-first, cross-platform native configurator that speaks directly to the stock keyboard over HID. Recognize the Nia87 and show its correct 87-key TKL layout automatically on every connection, including the first launch on a new computer. Replacement firmware, bootloader access, firmware dumping, and flashing are outside the current scope.

The user has confirmed that the Sharkfin web application works with this exact Nia87, but requires repeated TKL layout setup. This is user-reported compatibility evidence, not proof that all of its commands or wireless paths work. Treat Sharkfin as a behavioral reference only; no source, device tables, layout assets, or tests will be incorporated or translated.

### Plug-and-play acceptance criteria

- Ship an independently verified Nia87 board profile: physical geometry, key labels, configuration-slot mapping, supported layers and capabilities. A generic TKL picture alone is insufficient.
- Match the vendor collection, then verify board identity and firmware compatibility using known read commands before enabling writes. VID/PID and generic USB product strings are not unique model identifiers.
- Automatically load the built-in layout on a clean install without network access, a layout wizard, or imported vendor files. Later user customization is optional and persists locally.
- Distinguish physical key positions from current assignments: reading an already remapped keyboard must not rearrange the layout or misidentify the board.
- Reconnect, restart the application, and power-cycle the keyboard without losing recognition. Handle multiple devices explicitly rather than silently selecting the first.
- Unsupported revisions remain read-only with a clear explanation. A cached preference must never override a conflicting device identity.

### Connected-device observations

Windows PnP and an original Rust HIDAPI enumerator both see `3151:4015`, manufacturer `ROYUAN`, product `Gaming Keyboard`, USB release `0x0100` (not an established firmware version). Seven HID collections are exposed. Interface 2 has usage page `0xFFFF`, usage `0x0002`, matching the vendor bundle's configuration selector. Interface 1 also has a different vendor collection (`0xFFFF:0x0001`); it must not be selected accidentally. The Nia87 model identity still needs a verified protocol read.

Neither Wireshark nor USBPcap was found in the inspected standard installation directories or PATH. The user has authorized installing them if needed. Start with descriptor inspection and the supplied vendor bundle; use narrowly scoped USB captures when needed to establish command behavior. Installing a packet capture driver is unnecessary for enumeration and should not become a product dependency.

The [retailer's product page](https://stackskb.com/store/menel-nia-87-tkl/) states that ordinary keyboard operation already works on Linux and settings persist across operating systems. The missing support is the configuration application. It describes a conventional mechanical, tri-mode TKL, not a Hall-effect board.

Hard requirements for our implementation:

- No JavaScript, TypeScript, Node, Electron, Chromium, embedded browser, Tauri, or web UI in the native product or its build pipeline.
- Native Linux, Windows, and macOS applications; Windows is the currently connected hardware research host, with Linux configuration support a release requirement.
- No dependency on the OEM GUI, `iot_driver.exe`, vendor accounts, or vendor cloud for routine configuration.
- Original implementation with permissive dependencies. No incorporation or translation of GPL, LGPL, AGPL, MPL, or other copyleft code into the product.
- Inspecting the supplied vendor JavaScript as research evidence does not make it an implementation dependency. Keep those files out of distributable source and packages.

## What the supplied files establish

The fixtures are actually in `References/`. The workspace initially contained these two files and no application source or AGENTS.md was found.

| Fixture | Bytes | SHA-256 |
| --- | ---: | --- |
| `iot_v222.exe` | 3,859,072 | `6989AA433FB117343A93FF7ECFD2A4EF29E1F49F96BED817999B0DD5604F5C7A` |
| `Nia87_setup_2.1.97WIN20250717(1).zip` | 68,895,121 | `7AD0BF11D84D62BC00DF52AD76F2CFA5E28AA38F5F478C6B2E025FA1462E1242` |

Static inspection using installed 7-Zip established:

- The standalone IoT file identifies itself as an Inno Setup installer for `iot_driver`, company `Rongyuan, Inc.`. Its payload has not yet been unpacked.
- The ZIP contains a 68,903,632-byte NSIS installer. Its nested `app-32.7z` contains Electron/Chromium components, `resources/app/iot_driver.exe`, and an unpacked application directory rather than an ASAR-only payload.
- The application's package metadata identifies `Nia87 Driver` version `2.1.97`, with React, Electron remote, protobuf, and gRPC-Web dependencies. Dependency names alone do not establish which transport the Nia87 uses.
- The device registry inside `resources/app/dist/static/js/main_ccea61a6.js` contains the following factual values:

| Field | Entry A | Entry B |
| --- | --- | --- |
| Name | `yc3121_nia87_soc` | `yc3121_nia87_soc` |
| Display name | `Nia87` | `Nia87` |
| Registry ID | `-2147486047` | `2399` |
| VID | `12625` / `0x3151` | `12625` / `0x3151` |
| PID | `16401` / `0x4011` | `16405` / `0x4015` |
| Usage page / usage | `65535` / `2` | `65535` / `2` |
| Feature-report buffer length | `65` | `65` |
| `fnLayer` | `1` | `1` |

The matching image is named `company/company_Nia87/dev/yc3121_nia87_soc.png`. This is strong software evidence for a YC3121 target, but it is not a physical MCU identification. Registry IDs may include application-generated identifiers: do not treat either as a firmware-download ID until its semantics are traced. Do not assign wired/dongle roles to these PIDs yet. Likewise, confirm whether the 65-byte buffer includes a report-ID prefix using descriptors and captures.

Extracted research files are under `Research/extracted/`. No third-party open-source implementation was cloned or copied into the project.

## Architecture

Recommended stack: **Rust core and CLI, native egui/eframe GUI, HIDAPI transport**.

[egui/eframe](https://github.com/emilk/egui) supports native Linux, Windows, and macOS and offers MIT/Apache-2.0 licensing. Use only its native targets. It draws its own controls: native executable and rendering, but not the operating system's stock widget set. Validate keyboard navigation, accessibility, scaling, and idle resource use before committing to the GUI design.

[HIDAPI](https://github.com/libusb/hidapi) supports cross-platform HID access. Select its [BSD-style license option](https://raw.githubusercontent.com/libusb/hidapi/master/LICENSE.txt), audit the Rust binding and selected backend separately, and prefer Linux hidraw access. Avoid pulling in a libusb backend unnecessarily. Audit the actual target-specific dependency graph, including font and dialog dependencies; a permissive top-level license is insufficient. Existing OS services are distinct from code bundled into Byakko.

Proposed structure:

```text
crates/protocol/       Pure encoders, decoders, validated command definitions
crates/device/         Board identity, capabilities, serialized transactions
crates/transport/      HIDAPI transport and offline capture replay
crates/cli/            Enumeration, inspection, export, verified configuration
crates/gui/            Native egui application using the same device library
devices/nia87/         Independently verified board description and key positions
docs/                  Protocol, hardware, recovery, provenance, captures index
tests/fixtures/        Our sanitized captures and original expected results
tools/                 Rust/Python research tools; no Node toolchain
```

The GUI connects directly through the shared library. No localhost server or always-running daemon is needed. Use a worker thread for device I/O, a single transaction queue per device, cancellation and timeouts, and explicit disconnected/unsupported states. Match VID/PID plus usage/interface, verified device identity and firmware family; never authorize writes based on PID alone.

Use the OS's existing HID driver. Linux may need a narrowly scoped udev permission rule; running the GUI as root or granting access to every HID device is not the design. Do not replace the keyboard's normal input driver on Windows.

### How the web-interface requirement fits

The native application eliminates the need for the OEM web interface and IoT helper altogether. This is the default deliverable.

If retaining an actual browser interface is also required, there are two separate paths:

| Path | Consequence |
| --- | --- |
| Direct WebHID | Can avoid a helper where the board exposes an accessible vendor collection, but needs browser-side JavaScript API integration and a compatible browser. WASM does not eliminate that integration. Linux permissions still apply. |
| Compatibility bridge for the existing OEM page | Could be written in native Rust, but still requires a local helper and the OEM page's JavaScript. It does not satisfy a zero-helper goal. |

[Chrome's WebHID documentation](https://developer.chrome.com/docs/capabilities/hid) describes the JavaScript API and report access. Actual Nia87 browser accessibility remains untested. Exclude both browser paths from the initial scope to preserve the strict no-web-technology native requirement; do not promise the existing vendor site will work without modification.

## Implementation phases and completion criteria

### 1. Inventory and identify

Finish a reproducible extraction manifest with hashes for nested artifacts. Use a non-executing Inno extractor for the standalone installer. Compare its payload with the bundled helper; installer filenames do not prove payload versions match.

Collect USB descriptors in wired mode: interfaces, collections, report descriptors, report IDs, lengths, strings, and firmware version where a known query exists. On Linux, begin with `lsusb`, sysfs and hidraw enumeration. Enumeration must not send vendor configuration commands. Physical disassembly is not a prerequisite for stock-firmware configuration. Inspect the dongle separately during the later wireless phase.

**Done:** an evidence-backed device identity sheet, with observed facts separated from registry candidates. No guessed VID/PID rules or MCU assumptions become write permissions.

### 2. Recover the configuration protocol

Trace the Nia87-specific registry entry to its transport and command implementations in the supplied bundle. Investigate gRPC service definitions, loopback endpoint information, native helper imports and USB report handling. The network bridge is useful evidence, but the replacement should implement the underlying device protocol directly.

Capture controlled vendor-app actions on a Windows research installation using USBPcap/Wireshark, or equivalent USB capture through an isolated setup. This is a future research activity, not something performed during this planning task. Change one setting per capture: read identity, read profile, change brightness, restore brightness, remap one nonessential key, restore it, and then a short macro. Record initial state, exact UI action, packets, response and post-reconnect behavior. Keep personal typing out of captures.

Document report framing, prefix bytes, checksums and coverage, byte order, response matching, pagination, timing, flash commit behavior, profile selection, layer/key indices, macro storage and restore semantics. Distinguish configuration slots, USB HID usages, and physical matrix positions. Test whether replies are fresh and whether unsupported queries return stale data.

**Done:** a Nia87-specific protocol specification and paired original captures supporting each implemented command. Unknown opcodes are not probed by brute force; updater commands stay outside the configuration allowlist.

### 3. Deliver a read-only native CLI

`byakko devices` is now implemented and tested on the connected Windows host. `byakko inspect` and `byakko export` remain planned. Add only independently verified read transactions. Export a versioned backup with board identity, firmware revision, raw supported settings and decoded values. Explicitly mark settings that cannot be read reliably.

Build parser and encoder tests from our captures, including truncated reports, invalid lengths/checksums, unrelated replies, stale responses, timeouts and unplugging. Offline replay tests should exercise transaction behavior without hardware. Fuzz parsers offline, not a live keyboard.

**Done:** non-root Linux inspection and configuration export with the OEM software absent, no persistence writes, and normal typing intact. Validate Windows and macOS transport behavior as hardware becomes available.

### 4. Add bounded configuration writes

Start with one reversible lighting setting, then one key remap, then complete keymaps/profiles, then macros and per-key lighting. Preserve unknown bytes and unrelated fields. Implement read-modify-write where supported, validate ranges and capacities, serialize transactions, and avoid blind retries of non-idempotent writes.

Use an explicit Apply operation and coalesce changes. Establish actual device persistence and flash-wear behavior before adding frequent updates or animations. Acknowledgement alone is insufficient: check readback where reliable, observable behavior, power-cycle persistence and restoration. Restore only what was actually backed up; never advertise a full backup if fields are unreadable.

**Done:** change and restore settings on Linux, unplug the keyboard, and verify that intended onboard behavior persists on another host. Unknown device variants remain read-only.

### 5. Deliver the native GUI

Implement an original 87-key layout editor, supported Fn-layer controls, lighting, macro editor, profile import/export, device status, Apply and restore. Show capabilities established for the actual board; do not inherit features from unrelated OEM models. Keep protocol and timing logic in the core rather than duplicating it in widgets.

Package native builds with license notices and appropriately scoped Linux permissions. Test Linux Wayland/X11, Windows, and macOS; include suspend/resume, hotplug, concurrent OEM-app access, high-DPI and keyboard navigation. Verify the packaged dependency inventory has no web runtime or copyleft code incorporated into our distribution.

**Done:** offline configuration without vendor services, IoT helper, browser, or Node/Electron tooling; the Nia87 TKL layout loads automatically even with no existing user preferences.

### 6. Investigate 2.4 GHz configuration after USB works

Enumerate the receiver independently; do not assume candidate PID `4011` is its identity. Establish whether it relays the vendor configuration channel at all. Begin with known reads, correlate the attached keyboard identity, and test pairing changes, sleep/wake, reconnects, timeouts and device removal. A USB receiver's USB bus type does not itself distinguish it from a wired keyboard.

Only enable commands individually verified through the receiver, with measured pacing and acknowledgement behavior. Typing over 2.4 GHz does not imply configuration is supported. If the stock receiver cannot relay settings, explain that configuration requires USB and verify which USB-configured settings carry over to wireless use. Do not flash the receiver or keyboard to add support.

**Done:** either a tested per-feature wireless configuration matrix or a documented stock-hardware limitation. USB support ships independently.

## Relevant projects and source boundaries

| Project | Useful evidence | Reuse decision |
| --- | --- | --- |
| [sharkfin](https://github.com/dniminenn/sharkfin) | Closely related Rongyuan configuration work; protocol documentation and supported-device investigation | GPL-3.0; no source, tests, registry tables or UI assets incorporated. Also uses a web frontend/Tauri, outside our architecture. |
| [monsgeek-akko-linux](https://github.com/echtzeit-solutions/monsgeek-akko-linux) | Demonstrates native Linux HID configuration and a replacement bridge for a related OEM family | GPL-3.0; reference findings only. Mainly magnetic keyboards; no Nia87 compatibility assumed. |
| [ry5088-flasher](https://github.com/dot-agi/ry5088-flasher) | MIT project with firmware/recovery research | Candidate for audited permissive reuse only on matching hardware. Its own README excludes YC3121 devices; do not run its flasher against the Nia87. Its firmware is described as not yet fully brought up on hardware. |
| [RieGan/rongyuan-kb-software](https://github.com/RieGan/rongyuan-kb-software), [noaione/rongyuan-software](https://github.com/noaione/rongyuan-software) | Vendor software archaeology and naming context | Uploaded/extracted vendor software is not automatically reusable open-source code. No copying into our implementation. |

The [sharkfin author's research](https://dnim.dev/blog/royuan-keyboard-protocol) reports overlapping command numbers with different meanings across OEM families, misleading replies, and persistent lighting writes. Treat these as reasons to design Nia87-specific experiments, not as confirmed Nia87 behavior or a command table to translate.

Maintain a provenance ledger for every implemented command: our capture hash, experiment, fixture version, interpretation and confidence. Write original specifications from observed behavior. Do not translate GPL implementations into Rust, reproduce their structure or tests, or assume factual discoveries grant permission to copy documentation wholesale. Record the third-party documentation already consulted; this session is not a formal separated-person clean-room process.

Before taking any permissive code, pin a revision and audit its actual files, dependencies and origin. The vendor package's `ISC` metadata is not sufficient evidence that every bundled component is reusable. Keep proprietary fixtures, extracted code and firmware dumps private and outside release artifacts.

## First concrete work package

1. Preserve fixture hashes and create extraction/provenance manifests.
2. Finish helper extraction and trace only the Nia87 transport and identity path.
3. Completed: build and run a native Rust HID enumerator with no vendor commands.
4. Completed: enumerate connected `3151:4015`; next inspect its configuration report descriptor. Defer receiver identity until the wireless phase.
5. Create `docs/nia87-protocol.md` with confidence labels and original capture references.
6. Implement one verified identity/configuration read and an offline replay test.
7. Establish the Nia87 physical-position-to-slot mapping and bake an original board profile into the application; verify it against an already remapped keyboard and across restarts.

The next decision point is the first successful, verified native identity/configuration read from this exact board. The subsequent milestone is a USB configurator that recognizes the board and restores its visual layout automatically. Firmware replacement is outside this plan.

