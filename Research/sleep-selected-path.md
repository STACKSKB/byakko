# Nia87 selected sleep write path (facts only)

## Scope and provenance

- Bundle under review: ignored research artifact `Research/extracted/web-current/main_68eaf5ce.js`, identified by `Research/protocol-web-fn.md` as the current official web bundle (`main_68eaf5ce.js`, SHA-256 `3708DB6C84EDCE9CA8A2424D744A9EE8A1EF66A267F6F3ECA019711D9BF3FECA`).
- The Nia87 registry entries are `yc3121_nia87_soc`, display name `Nia87`, VID `12625`, PIDs `16401` and `16405`, feature report length `65`, and `fnLayer: 1`.
- The factory switch for `yc3121_nia87_soc` selects `Emt`; the minified declaration shows `Emt` extending `YHe`. `Emt` sets `defaultMatrix`; no sleep setter override is present in the `Emt` declaration examined. The selected sleep method therefore resolves through its inherited lineage.

## Selected setter and field offsets

- The selected sleep command is `0x12` (`FEA_CMD_SET_SLEEPTIME`); getter is `0x92`.
- The current inherited setter allocates a zero-filled 64-byte feature payload, places command `0x12` at payload byte 0, and encodes unsigned little-endian seconds as follows:

  | Payload bytes | Field |
  | --- | --- |
  | 8–9 | `time_bt` |
  | 10–11 | `time_24` |
  | 12–13 | `deepTime_bt` |
  | 14–15 | `deepTime_24` |

- The setter calls `writeFeatureCmd(report, 0)` (explicit checksum mode value `0`, documented by the bundle as `BIT7`) and then the inherited common delay continuation.
- These are feature-payload offsets. The host HID report has a separate report-ID byte before the 64-byte payload because the registry reports length 65.

## UI caller

- The settings UI exposes normal and deep timers for `sleep_24` and `sleep_bt` from the Nia87 descriptor. Each input/check action updates one field in the sleep object and calls the settings store's `setSleepTime`.
- The store method checks the current device and invokes `this.currentDev.setSleepTime(value)`. For Nia87 this dispatches to the selected `Emt` instance and its inherited setter above.
- The Nia87 descriptor supplies normal ranges of 1–60 minutes and deep ranges of 10–60 minutes for both transports; zero is represented by the UI's no-sleep state where supported by the descriptor.

## Conflict and uncertainty

- `docs/settings-protocol.md` describes an older/different Nia87 path as class `Pft` and assigns sleep fields to bytes 1–8. That is not the current registry-selected class path established above.
- The current bundle setter writes bytes 8–15 and passes checksum mode `0`. The bundle does not expose the native helper's final checksum placement or any native transformation at the HID boundary. Therefore byte-7 checksum claims from the older bytes-1–8 description cannot be applied to this current setter.
- This note records static bundle/caller facts only. It does not claim a device write was performed or that native checksum behavior was independently validated.
