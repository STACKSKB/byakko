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
subscription. The UI identified Nia87 over USB. The initial analysis included
eight `sendMsg` and eight `readMsg` calls; those are startup activity, not an
idle polling rate.

Measurement correction: the first 30.547-second sample used directory-entry
file sizes. Windows retained stale lengths for the open capture files, so the
initial zero-traffic claim from those samples is withdrawn. Opening each file
and querying its handle length revealed the ongoing traffic. The CPU samples
below are independent of that mistake. Do not use the original directory-size
samples as evidence of idle traffic or absence of notifications.

This observes the frontend/helper boundary. It does not establish the absence
of USB polling inside the helper. The user clarified that the only physical
onboard action was factory reset: the other lighting-mode changes were made
in the official software, not through physical mode shortcuts.

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

## User's reset and official lighting selections

Re-decoding the actual open streams after the user's experiment produced 119
RPC exchanges, including 42 `sendMsg` and 23 `readMsg` calls. The vendor stream
contains a 65-byte message beginning `05 0D 00 00`, which the frontend decodes
as reset, alongside sixteen start/stop messages beginning `05 0F 01 00` or
`05 0F 00 00`. No lighting-change event was present in this capture; physical
lighting shortcuts were not tested.

The main request stream contains a batch of eight keymap-page reads plus
profile/settings/lighting getters between the earlier and later lighting
writes. The lighting getter in that batch returned mode 4 (Wave), following
an earlier mode-13 (custom picture) observation. Static reset handling at
approximately 15751956 waits two seconds and calls `loadDeviceInfo(false,true)`.
Together these support reset-triggered internal refresh on this Nia87. They
do not contradict the user's report that no visible live update was apparent:
the rendered result was not captured, and the proxy has no cross-stream
per-message timestamps to establish precise event/read latency.

The user's official UI selections sent global setters with mode IDs 8, 21, 2
and 13. Both observed custom-picture operations sent `07 0D ...` (activate
mode 13) followed by seven `0C ...` picture pages. Thus the actual selected-mode
write precedes the color data in these operations; no polling is needed for
that explicit user intent to take effect.

Static setter inspection distinguishes two cases:

- Global RGB edits clone the cached lighting setting and send effect, speed,
  brightness, options and RGB together. This can reactivate the cached mode
  after an unobserved onboard change.
- The custom-layer handler activates mode 13 only if its cached mode/selector
  differs, then writes picture pages. The picture-page setter alone does not
  activate the mode. Do not infer that every per-key color edit reasserts mode
  13, or that the official app cannot suffer a stale-cache case.

Relevant bundle offsets: RGB handler ~16586100, global setter ~7708420,
custom-layer handler ~16627000, picture setter ~7695305. No additional keyboard
writes or physical reset were performed by the investigating agent.

## Physical Fn+Right Ctrl follow-up

The user subsequently cycled modes with physical Fn+Right Ctrl while a
timestamped monitor watched the existing capture files (no keyboard polling).
Between 11:49:38 and 11:51:18 UTC it recorded **19 lighting notifications and
19 matching lighting getter exchanges**. The notification payload starts
`05 04 <mode> 00`; observed mode IDs were
`1,2,3,4,5,6,7,8,9,10,11,19,12,14,15,16,17,18,1`.

For every notification, the official frontend sent one `87` lighting getter
and obtained the corresponding mode in its response. This activity used no
keymap, macro or settings getter and no global-lighting/picture setter. There
were also 38 `changeWirelessLoopStatus` calls and associated start/stop events.
The lighting-getter request followed the mode notification by approximately
0.73–0.86 seconds (median 0.75 seconds). These are first-observed file-append
times from a 100 ms local-file monitor, not precise USB timings.

A separate corrected idle sample, 17:16:29.216–17:16:59.544 IST, checked lengths
through freshly opened file handles and found no additional request/response
bytes. This replaces the invalid directory-metadata measurement above.

This physically confirms the Nia87 notification/targeted-read path through the
official helper. It does not establish that the visible UI reflected every
mode change, nor yet identify the native HID input descriptor. The evidence
supports an event-driven replacement for Byakko's repeated full-state cycle.
The user need not repeat the mode-shortcut experiment.

Local evidence: `physical-mode-monitor.jsonl`, `idle-handle-samples.json` and
`rpc-analysis-physical-final.json` in the capture directory. The timestamped
monitor was stopped after the experiment.

## Additional physical controls capture

The user completed the manual-derived control table in a second capture under
`Research/captures/onboard-controls-20260927/`, excluding Fn+V. Native official
startup was captured separately from the physical actions. The timestamped
monitor observed 18 feature-change notifications (plus start/stop events),
followed by 12 lighting getters and four keyboard-option getters. All feature
reports sent in this capture were getters; no keyboard setters were issued by
the agent or official frontend during these physical tests.

| Physical control / observed change | Vendor message prefix | Official follow-up |
| --- | --- | --- |
| Brightness change | `05 06 03 00` | Lighting `87`, brightness 3 |
| Additional animated-mode selection | `05 04 02 00` | Lighting `87`, mode 2 |
| Speed down/up | `05 05 03 00`, `05 05 02 00` | One lighting `87` each |
| Four color changes | `05 07 00..03 00` | One lighting `87` each |
| Static shortcut | `05 04 01 00` | Lighting `87`, mode 1 |
| Windows-key lock/unlock | `05 03 01 01`, `05 03 00 01` | **No settings getter** |
| Power saving on/off | `05 03 01 09`, `05 03 00 09` | One keyboard-option `86` each |
| Mac/Windows switching | `05 03 02 02`, `05 03 00 02` | One keyboard-option `86` each |
| Custom selections Z/X/C | `05 04 0D 00`, `05 07 10 00`, `05 07 20 00` | One lighting `87` each; mode 13, selectors 0/1/2 |

Only one brightness-change event was captured, so this does not establish both
brightness directions despite the requested test sequence. Power and system
mode returned to their original states in the readbacks. Lock/unlock is
established by the notification pair, not an independent settings readback.
Custom selections caused no picture-page reads: the official getter returned
the lighting selector only, not the selected custom slot's actual colors.

The shared vendor decoder handles settings notification masks 0/2 (system),
4/9 (power) and 8 (Fn configuration), but does not dispatch mask 1 (Windows-key
lock). This explains the missing official refresh despite receiving both
notifications. Byakko should route the observed lock event to its settings
observation rather than inherit that omission. Selector changes also need the
existing selector-dependent picture invalidation/read rules.

Evidence is the untouched RPC streams, `analysis.json` and
`physical-controls-monitor.jsonl`. The monitor was stopped after completion.
Fn+V recording/save remains untested, as do wireless/pairing controls. No
additional physical input is required for the controls already captured.

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

The captured RPC reset message establishes delivery through the helper; it does
not by itself prove the underlying HID collection or report format. Before
implementing native observation, verify the input collection's live descriptor,
report ID/length, actual HID event bytes and any enable prerequisite.
Associate it with the same physical keyboard through a common device ancestor
(Windows) or USB sysfs ancestor (Linux), rather than replacing interface digits
in a device path. Use a cancellable input listener, bounded coalescing and typed
feature-change events. Do not put the official helper, RPC stream or vendor
JavaScript in the product, or infer raw HID layout from the RPC wrapper.

No product source or executable was changed by this profiling investigation.
The vendor bundle was restored byte-for-byte (SHA-256
`220C75F28257DDE0F665A7FE41880D11DD859333B31FD608B26FFDAD1F958217`).
The physical follow-up above completes the requested observation check.

## Authorized native implementation follow-up

The user subsequently requested replacing polling with these events. A read-only
Windows HID capability probe established that the sibling collection is
**four bytes**, report ID **5**, with three 8-bit fields; it is not the 65-byte
RPC payload. The event collection has input length 4, output/feature length 0;
the configuration collection has input length 0 and feature length 65. Both
resolve to the same physical USB composite ancestor. A pending 100 ms input
read returned no event while idle, and cancellation/drain completed promptly.
The probe's whole test completed in 0.18 seconds without feature reports.

The native listener retains one overlapped Windows input request across timeout
waits. Linux uses bounded hidraw input readiness and discards unrelated report
IDs. The desktop consumes a wake-driven, coalesced mailbox with connection
generation tags. Only queued events schedule debounce work; no notifications
means no periodic feature read. Connected enumeration is also paused while the
listener is healthy. No vendor helper or JavaScript is in the product path.

Linux compilation does not establish hardware support: the packaged udev rule
currently grants only the configuration node. The separate input interface's
complete descriptor and narrowly scoped access grant still require Linux
hardware acceptance. Listener failure is visible and leaves manual refresh
available rather than silently reinstating feature polling.
