# Native keyboard tester

The keyboard tester shows key presses and releases received by Byakko. It is a
local input diagnostic: it does not configure the keyboard, write to it, or
send input over the network. Results are held in memory for the current run;
the tester does not save or export them.

## Choose an input source

Use **Refresh** to update the available native input devices, then
choose a source from the source dropdown. The source is independent of the
keyboard selected for configuration. Hover **Input** for the full selected
device identifier; compact source labels retain an endpoint or instance suffix.

- **App window · all keyboards** receives physical key codes delivered to
  Byakko's window. Events have no keyboard identity, so this mode cannot tell
  which attached keyboard produced a key.
- A **native keyboard source** listens to the selected operating-system input
  device and provides source-specific events. On Linux, the account
  must have read access to that input device. Configure normal system device
  permissions for the account; Byakko does not require running the whole app
  with `sudo` and does not grab the device exclusively.

The tester starts paused. Choose a source and select **Start** to begin.
**Pause** ends the current capture. Capture also stops when the window loses
focus, the user leaves the tester, closes the window, or presses Shift+Tab.
Start it again explicitly; it does not resume automatically. **Reset**
clears the current in-memory results and pauses the test. The tester retains
up to 128 recent edge
interval samples and 32 recent chatter candidates. It also keeps the total
candidate count for the run until results are reset.


Release held keys before starting a test. Windows conservatively suppresses
keys already held when capture starts, using global OS key state; it cannot
attribute that initial held state to one keyboard. Linux snapshots the selected
evdev device with EVIOCGKEY. Both native sources exclude these initial holds
until their first release.

Collection stops when Byakko loses focus, so the tester does not continue
observing input in the background. Window input receives events delivered to
Byakko's window; a native keyboard source receives events only while the tester
is focused and the test is running. Neither source records input to disk.

## Reported host keys and layouts

Coverage, highlights and counts describe the host key usages reported by the
input source. Firmware can map a physical Caps Lock switch to Escape, for
example; that actuation highlights the reported Escape usage. The tester cannot
infer which switch was actuated and does not read the configurator's keymap.
ANSI, ISO and JIS presets cover full size, TKL, 75%, 65% and 60% geometries.
Choosing JIS initially selects Japanese legends; legends remain a display choice.
Presets are representative layouts rather than an exact model for every board.

Shift+Tab publishes Pause on its first chord and runs Iced's backward focus
operation. Other test keys remain captured so they do not activate controls.
Focus traversal follows the focusable controls provided by Iced.

On Windows the sampler temporarily owns the process-wide Raw Input keyboard
registration, retaining normal window key delivery. It stops if it detects an
owner change and restores the previous registration only while its hidden
window still owns it. Win32 provides no atomic conditional restoration, so a
concurrent toolkit registration can race that final check. The current Iced
integration registers through winit at event-loop creation and does not call
winit's later device-listening registration API. This limits the observed
lifecycle; it does not prove race-free restoration under every future caller.

## Understand the timing

Intervals are differences between received event timestamps. They are not the
USB polling rate, switch travel time, or end-to-end physical latency. Hover
**Timing** for the timestamp source and measurement limits.

On Windows, **Instant** means the time Byakko dequeues the native Raw Input
message (or receives an Iced event in window-input mode), captured with a
monotonic application clock. Raw Input delivers
`WM_INPUT` messages to a registered application; it does not give this tester a
hardware switch-edge timestamp. The value therefore includes time between
device reporting, operating-system delivery, and application receipt.

On Linux native input, Byakko requests CLOCK_MONOTONIC timestamps from evdev
with EVIOCSCLOCKID; if that request fails, the source cannot start. This
explicit clock selection matters because evdev's default event clock is not
guaranteed to be monotonic. EV_KEY values distinguish press (1), release (0),
and autorepeat (2). The timestamps are kernel event timestamps, but they do
not timestamp the physical switch contact or USB bus transaction. Window input
is an application-receipt measurement, not an evdev timestamp.

A USB interrupt endpoint's `bInterval` descriptor declares its requested
service interval, interpreted according to USB speed. For USB 2.0 high-speed
interrupt endpoints, the interval is expressed in powers of two microframes:
`2^(bInterval - 1)` × 125 microseconds. Full-speed interrupt intervals use
1 ms frames. This is a declared scheduling opportunity, not evidence that a
transaction occurred at every opportunity, nor a measurement of actual device
polling. The tester inspects a descriptor snapshot when you explicitly choose a
native input source. Linux follows that event device's exact sysfs USB interface
ancestor, active configuration and alternate setting. Windows resolves the exact
Raw Input device-interface path to its USB ancestor, then requires its driver
key to match one hub port before issuing standard descriptor GET requests.
Neither path matches another keyboard by vendor/product ID or changes USB
configuration. Unsupported, inaccessible or ambiguous sources show the reason
that descriptor metadata is unavailable.

Hover **USB declared** to see each interrupt IN endpoint separately with the
USB speed, interface, alternate setting, endpoint address and raw `bInterval`.
This is interface metadata; multiple HID collections can share the endpoint.
Windows declines interfaces with multiple alternate settings when the active
setting cannot be established. Metadata inspection is independent of Start and
retained while testing is paused; choosing the source again requests a fresh
snapshot. There is no periodic descriptor polling. Stale or duplicate results
are rejected, and a removed source invalidates its snapshot.

The Windows inventory and descriptor sizes are bounded. Hub requests currently
use synchronous OS calls on the task executor: a stalled driver can retain that
worker because these calls have no per-request timeout. A Windows Nia87 2.4 GHz
receiver check resolved the selected keyboard input to full-speed interface 0,
alternate 0, interrupt IN endpoint 0x81, with bInterval 1: a declared 1.000 ms
service interval. Linux sysfs behavior remains to be verified on hardware.

Host scheduling can differ from the descriptor's declared period. A USB radio
receiver's descriptor describes the receiver's host endpoint, not the wireless
link to its keyboard. Even a successfully inspected descriptor does not
establish observed bus transaction cadence or physical actuation latency.

## Chatter candidates

The default chatter threshold is **20 ms** and can be adjusted in the tester.
A candidate requires a release followed by a press of the same key within the
threshold. Changing the threshold affects future observations; use Reset
for a fresh comparison. Autorepeat events are excluded; they are not
switch release/press
cycles. A candidate is only a timing pattern: deliberate rapid tapping and
switch chatter can look identical in the event stream. Confirm a suspected
switch issue with a physical test before drawing a hardware conclusion.

Software-generated key hammering, operating-system typematic repeat, and
macro replay can exercise event handling, but none is a physical actuation
reference. A tester run alone cannot establish switch debounce quality,
physical actuation latency, or a device's USB polling rate.

## Physical latency measurement

For a physical actuation-to-report measurement, record both the physical switch
reference and the corresponding USB input report on a shared analyzer clock.
Define the physical reference before collecting data, for example as the
electrical contact edge, an optical beam crossing at a stated position, or a
force sensor crossing a stated threshold. Match each reference edge to its
reported key transition. If clocks differ, calibrate their offset and drift
over the measurement period and include the residual synchronization error.

Record the device, connection, operating conditions, reference definition,
measurement tools, timestamp sources, clock calibration, and uncertainty. Use
enough repeated actuations to show the distribution, and report the sample
count, median, 95th percentile, range, and measurement error budget. Do not
present the result as universal device latency: it applies to the tested
device, setup, and operating conditions.

Software USB captures such as USBPcap and Linux usbmon observe host-stack
transfers/URBs rather than every USB packet and transaction. They can help
correlate completed reports, but absence of a report does not show whether an
empty poll or NAK occurred. Measuring actual bus transactions requires a USB
bus analyzer that captures those transactions.

## References

- [Microsoft: USBView descriptor and hub query sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-driver-samples/usbview-sample-application/)
- [Linux kernel: USB sysfs ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-bus-usb)
- [Microsoft: Raw Input Overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input)
- [Linux kernel: Input event codes](https://docs.kernel.org/input/event-codes.html)
- [Linux kernel UAPI: EVIOCSCLOCKID](https://github.com/torvalds/linux/blob/master/include/uapi/linux/input.h)
- [Linux kernel: Input event interface and timestamps](https://docs.kernel.org/input/input_uapi.html)
- [USB-IF: USB 2.0 Specification](https://www.usb.org/document-library/usb-20-specification)
- [Wireshark: USB capture setup and software URB capture limits](https://wiki.wireshark.org/CaptureSetup/USB)

This Byakko-owned documentation is licensed under GPL-3.0-or-later.
