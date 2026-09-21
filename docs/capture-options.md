# Official-app capture options

This note records bounded static inspection of the extracted Nia87 Driver 2.1.97
build. No process, device, or UI was started while collecting these findings.

## Recommended capture point: localhost RPC

The renderer creates a gRPC-Web client for `http://127.0.0.1:3814` (in
`dist/static/js/main_ccea61a6.js`, byte offset about 1,565,043). The generated
client exposes these unary methods:

```
/driver.DriverGrpc/sendMsg
/driver.DriverGrpc/readMsg
```

The normal wrapper constructs `SendMsg` with four protobuf fields:

| field | type | meaning in the extracted client |
| --- | --- | --- |
| `devicepath` | string | HID device path |
| `msg` | bytes | command or feature payload supplied by the UI |
| `checksumtype` | enum | checksum mode passed to the helper |
| `dangledevtype` | enum | device/dongle type, defaulting to `NONE` |

`ReadMsg` carries the device path, and `ResRead.msg` is returned as bytes. The
same client also exposes `sendRawFeature` and `readRawFeature`; these are useful
for separating a raw feature-report path from the ordinary helper path. The
protobuf field layout and wrappers are around offsets 1,393,809–1,397,173.

This boundary captures the requests selected by the official UI and the replies
returned by the helper, before helper-side checksum handling and HID I/O. A
future controlled capture should record timestamps, endpoint, request body, and
response body, then decode the gRPC-Web/protobuf envelope. Preserve `msg` and
`checksumtype` separately: the request bytes are the app/helper contract and may
not yet be the final USB report in every checksum mode.

DevTools is opened automatically only when `app.isPackaged` is false. The
packaged main process does not call `openDevTools`, and static inspection found
no `remote-debugging-port` switch. It does append only `no-sandbox` and
`disable-background-timer-throttling` (`resources/app/main_dist/main.js:9706-9707`).
If a future run uses Chromium DevTools Protocol, start the packaged app with a
localhost-only debugging port before another instance is running, then capture
only requests to `127.0.0.1:3814`. The app's single-instance lock means a later
launch may merely focus the existing instance.

## How the official app starts the helper

The renderer bundle (`dist/static/js/main_ccea61a6.js`, around byte offset
17,796,295) does the following on Windows:

1. `tasklist`/`taskkill /f /t` is used to stop existing processes whose names
   contain `iot_driver`.
2. The helper is copied from the app directory into Electron `userData` under
   a versioned name such as `iot_driver_<version>.exe`; stale versioned copies
   are removed.
3. The Windows copy is started with `@electron/remote`.shell.openPath(path),
   with no arguments and no stdout/stderr capture. The macOS branch uses
   `spawn(path, [], { detached: true, windowsHide: true, stdio: "ignore" })`.

Consequently, there is no verified helper command-line logging flag, output file,
or environment-variable capture switch in this build. The bundle contains no
`--log`, `--verbose`, `RUST_LOG`, `remote-debugging-port`, or helper log-path
literal. The extracted helper is a 32-bit PE (`Machine 0x014c`) with PE
subsystem 2 (`WINDOWS_GUI`), which also makes inherited-console output an
unreliable capture route.

## What the helper binary does and does not expose

`resources/app/iot_driver.exe` is 6,698,048 bytes. Bounded ASCII string
inspection found the following relevant evidence:

| evidence | offset (decimal) | interpretation |
| --- | ---: | --- |
| `HidD_SetFeature`, `HidD_GetFeature` | 5,585,104 / 5,585,120 | Windows HID feature-report APIs are present |
| `hid_send_feature_report` | 6,309,462 | hidapi feature-report API is present |
| `127.0.0.1:3814` | 5,122,624 | helper listen address is embedded |
| `src\\grpc_server\\mod.rs` | 5,052,384; others | Rust source path metadata for the RPC server |
| `Got a request:` | 5,140,616 | an embedded diagnostic format string |
| `Received data from`, `data ===>` | 5,047,920 / 5,047,992 | embedded diagnostic strings near `src\\dj_dev_api\\ble_hid.rs` |
| `sendddd`, `sender errrrr` | 5,049,509 / 5,052,440 | embedded error/diagnostic strings |

These strings show that some diagnostics were compiled into the helper, but do
not establish an enabled subscriber, a packet dump, or a controllable log
level. `tracing-core` dependency paths are present, while no
`tracing_subscriber`, `RUST_LOG`, or log-file path was found. The
`Received data` strings are adjacent to BLE helper source metadata, so they are
not evidence that ordinary USB feature reports are printed.

The helper's embedded support table includes the keyboard identity
`vid = 0x3151`, `pid = 0x4015`, `usage = 0x2`, `usage_page = 0xffff`,
`interface_number = 2` (around offset 5,075,327). If a USB capture driver is
already available, filter control transfers for this interface and inspect
HID `SET_REPORT`/`GET_REPORT` payloads. USB capture observes the final bytes
after helper checksum/report processing, while localhost RPC capture preserves
the app-side request and is easier to correlate with a particular UI action.

## Practical order for a future capture

1. Capture the localhost RPC first, using an identity read or another harmless
   read to validate protobuf decoding and timing.
2. Correlate `sendMsg`/`readMsg` (and, where used, raw-feature calls) with the
   app-side action and retain the original request bytes.
3. Use USBPcap/Wireshark only if final HID bytes or helper checksum expansion
   must be verified. No USBPcap installation or driver/filter change was made
   during this inspection.

