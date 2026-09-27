# Official onboard-change observation, 2026-09-27

The user reports periodic stutter and disabled controls during Byakko's repeated
reads. This investigation profiles the existing native Nia87 Driver 2.1.97 and
inspects its notification handling. It does not establish a working replacement
listener in Byakko.

## Native capture

The official Electron application was launched against its own local helper.
The research copy of its renderer's single localhost endpoint was temporarily
redirected from port 3814 to the existing passive RPC proxy on port 3815. No
vendor behavior or protocol implementation was copied into the product. The
proxy forwards unchanged traffic and saves request/response bytes. Raw captures,
the exact original bundle, and process/file-size samples are locally retained in
`Research/captures/onboard-profile-20260927/` (ignored research artifacts).

Startup traffic included one `watchVender` subscription and one `watchDevList`
subscription. The UI identified Nia87 over USB. After settling, the interval
17:07:37.713–17:08:08.260 IST (30.547 seconds) contained **zero additional bytes
in any request or response capture**. In particular, there were no periodic
frontend getter calls and no notification arrivals during that idle interval.
The initial capture included eight `sendMsg` and eight `readMsg` calls; those
are startup activity, not an idle polling rate.

This observes the frontend/helper boundary. It does not establish the absence
of USB polling inside the helper. A physical brightness down/up check has been
requested to correlate an actual onboard change with stream data and the
subsequent getter. Until that check is captured, actual Nia87 notification
delivery remains unverified.

CPU measurements use differences in Windows process CPU time over that same
interval, with 32 logical processors. The four Electron processes consumed
14.781 CPU seconds in total (48.39% of one logical processor, 1.51% of machine
capacity). The helper consumed 12.516 CPU seconds (40.97% of one logical
processor, 1.28% of machine capacity). Final summed working sets were about
314.0 MiB for Electron and 36.3 MiB for the helper. Summed working sets can count
shared pages more than once. The proxy consumed no measurable CPU at the
process counter's resolution and had a 4.6 MiB working set.

These numbers are a bounded native idle sample under instrumentation, not a
matched Byakko/Sharkfin efficiency comparison or a system-stutter trace. The
official app's lack of frontend polling does not make its idle CPU cost zero.

## Notification handling found in the official implementation

Evidence locations below are approximate character offsets in the locally
extracted `resources/app/dist/static/js/main_ccea61a6.js` bundle:

- Around 2620890, initialization subscribes to `watchDevList` and `watchVender`.
  The vendor-message consumer examines bytes 1–3 and coalesces events for
  500 ms before dispatch.
- Around 15691193, the device store handles lighting notifications with
  `getLightSetting`, not a complete keyboard reload. This targeted read still
  enters the official app's busy/idle state. Side-lighting events have an
  additional one-second debounce.
- Profile changes obtain the current profile and load its configuration.
  Windows/Mac and power-mode notifications read keyboard options; sleep changes
  read sleep time. Fn-configuration events use their value to select the Fn
  index. The shared event vocabulary also includes reset and report-rate events;
  this is a multi-device frontend and is not evidence that Nia87 emits every
  event type.
- Around 15748636 and 17202634/17272538, device selection/reset and an explicit
  read-current-configuration action initiate fuller loads.
- Around 17807193, Electron show/hide changes the helper's wireless-loop lock
  through `changeWirelessLoopStatus`. It does not unsubscribe from the vendor
  stream, and show does not itself reload the entire configuration.

No periodic full-state observation timer was found in this path. Unrelated
timers serve host effects, calibration, progress, weather and other devices.

## Why Byakko's current approach is disruptive

The two-second idle subscription starts an observation cycle using ordinary
session read commands. Those commands mark the session busy; views disable
controls while it is busy. The visible feature is read first, followed by other
loaded features. Therefore disabling controls is a direct consequence of this
implementation, not evidence of a keyboard firmware fault.

For the loaded keymap, settings, lighting and selected macro, established getter
pacing alone totals at least 810 ms: 540 ms for keymap, 120 ms for settings,
30 ms for lighting and 120 ms for the selected macro. Applicable per-key colors,
enumeration, opening and I/O add further cost. This is a code/pacing calculation,
not a measured wall-clock cycle or proof of the reported system-wide stutter.

The user requested state observability across onboard functions. They did not
request a two-second polling interval; that interval was an implementation
choice and has failed user acceptance.

## Replacement direction and remaining evidence

Replace the recurring full-state timer with selected-device notifications and
coalesced reads of the affected feature. Device work should remain serialized,
but a refresh should not blanket-disable navigation and unrelated editing.
Preserve in-flight user edits and the existing correlated completion/conflict
rules. A deliberate read remains useful when notifications are unavailable.

The helper's embedded support configuration names a separate vendor-event
collection: usage page `FFFF`, usage `1`, interface `1`. Byakko's selected
configuration collection is `FFFF/2`, interface `2`, and is feature-only. A
listener cannot simply read input from the existing feature handle.

Before implementing native observation, verify the input collection's live
descriptor, report ID/length, actual event bytes and any enable prerequisite.
Associate it with the same physical keyboard through a common device ancestor
(Windows) or USB sysfs ancestor (Linux), rather than replacing interface digits
in a device path. Use a cancellable input listener, bounded coalescing and typed
feature-change events. Do not put the official helper, RPC stream or vendor
JavaScript in the product, or infer raw HID layout from the RPC wrapper.

No product source or executable was changed by this profiling investigation.
The vendor bundle was restored byte-for-byte (SHA-256
`220C75F28257DDE0F665A7FE41880D11DD859333B31FD608B26FFDAD1F958217`).
The already-running official app retains its loaded capture endpoint; it and
the passive proxy remain open for the requested physical brightness check.
