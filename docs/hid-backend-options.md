# HID backend options

## Why the current binding needs a decision

The project pins `hidapi 2.6.7` with its native Windows and Linux backends. The
crate's package metadata and root `LICENSE.txt` say MIT. Its `build.rs`, however,
has a file-specific GPLv3-or-later notice, and Cargo compiles and executes that
script even when the selected native backend does not compile the bundled C
library. [Upstream's current build script](https://github.com/ruabmbua/hidapi-rs/blob/main/build.rs)
still carries that notice. The bundled C HIDAPI project separately offers a
[choice of GPLv3, BSD-style, or original terms](https://github.com/libusb/hidapi/blob/master/LICENSE.txt);
that choice does not clarify the Rust build script's file-specific notice. This
is a license provenance mismatch, not a conclusion about distribution law. A
project requirement of incorporating no GPL source favors removing this crate.

## Required capability

`src/device.rs` uses enumeration, a path-based open, manufacturer/product and
interface metadata, one raw report descriptor diagnostic, and 65-byte feature
report set/get. `src/main.rs` also uses enumeration for its `devices` command.
The working Nia87 path is VID `3151`, PID `4015`, collection usage page `ffff`,
usage `0002`, with a leading zero report-ID byte in host feature buffers. The
protocol layer, GUI, and storage can stay separate from the HID adapter.

| Route | Fit | Cost and uncertainty |
| --- | --- | --- |
| Original Windows/Linux native adapter | Best fit for narrow synchronous feature reports and no GPL-source requirement | Small amount of platform-specific `unsafe`/FFI, review and device validation needed. |
| [`async-hid 0.5.3`](https://docs.rs/async-hid/latest/async_hid/) | MIT, native Win32 and hidraw, feature-report handles | Async API integration and an upstream planned redesign of feature handles; requires fresh dependency/license and device audit. |
| Newer or forked `hidapi` | Minimal source changes | Current upstream still has the GPL-marked build script; does not meet the strict source-provenance goal without an independently licensed rewrite. |

## Native adapter design

Keep a small internal interface: `enumerate() -> Vec<Candidate>`, `open(path)`,
`send_feature(&[u8; 65])`, `get_feature(&mut [u8; 65])`, and optional
`report_descriptor()`. Enforce the VID/PID and vendor-collection filter before
any protocol operation. Own OS handles with RAII, check returned lengths and
errors, and keep `unsafe` within the platform modules.

On Windows, Microsoft's [documented enumeration sequence](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/finding-and-opening-a-hid-collection)
uses `HidD_GetHidGuid`, `SetupDiGetClassDevs`, `SetupDiEnumDeviceInterfaces`,
`SetupDiGetDeviceInterfaceDetail`, and `CreateFile`. Query VID/PID with
`HidD_GetAttributes`; obtain collection usage and feature-report length with
`HidD_GetPreparsedData` and `HidP_GetCaps`. `HIDP_CAPS.FeatureReportByteLength`
[includes the report-ID byte](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/hidpi/ns-hidpi-_hidp_caps).
Use [`HidD_SetFeature`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/hidsdi/nf-hidsdi-hidd_setfeature)
and [`HidD_GetFeature`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/hidsdi/nf-hidsdi-hidd_getfeature)
with exactly that length; report ID zero occupies byte 0. Read strings with
`HidD_GetManufacturerString` and `HidD_GetProductString`. Interface number is
display-only and can be derived from the Windows device path when available;
usage page/usage is the decisive collection filter. Microsoft's ordinary
user-mode collection API supplies opaque *preparsed* data, not a direct raw
report-descriptor getter, so the existing descriptor command may need an
explicit Windows `unsupported` result or a separate diagnostic mechanism.

On Linux, enumerate `/sys/class/hidraw`, identify the HID device and its USB
parent through sysfs attributes, then open only the corresponding `/dev/hidraw*`
node. The kernel [documents](https://docs.kernel.org/hid/hidraw.html)
`HIDIOCGRAWINFO`, `HIDIOCGRDESCSIZE`, `HIDIOCGRDESC`, `HIDIOCSFEATURE`, and
`HIDIOCGFEATURE`; it also documents the leading report-number byte for feature
ioctls. The raw descriptor is available through hidraw or the
[`report_descriptor` sysfs attribute](https://docs.kernel.org/hid/hidintro.html).
Parse the top-level collection usage to avoid opening another interface of the
same keyboard. Retain the narrowly scoped udev rule in
`docs/linux-readiness.md` for runtime permission.

## Proposed sequence

Implement the Windows adapter first and verify enumeration, the exact selected
collection, descriptor capability handling, and read-only 65-byte feature
reports on the known keyboard. Then integrate the existing guarded write path
without changing protocol framing. Implement the Linux adapter from the kernel
interfaces, cross-build it, and validate on a real Linux installation before
claiming Linux runtime support. The Windows portion is roughly a half to one
engineering day; the Linux portion is roughly another half to one day, plus
hardware testing. Those are effort estimates, not validation results.
