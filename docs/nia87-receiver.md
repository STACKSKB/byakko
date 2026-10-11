# Nia87 2.4 GHz receiver

Byakko's receiver path is a transport relay for the paired Nia87 keyboard. The
keyboard still uses its ordinary Nia87 feature commands; the receiver adds
status, target selection and reply-release reports around them. This is not a
new Rongyuan command family and does not establish compatibility with another
receiver or paired board.

## Select and read

On the tested Windows setup, choose the `3151:4011` receiver configuration
collection (`MI_02`, usage page `FFFF`, usage `2`). Its feature report is 65
bytes in the native HID API, including the leading report-ID byte. The paired
keyboard was identified as board `0x095f`, firmware `0x0100`,
profile 0. Do not infer pairing or write support from the receiver VID/PID
alone. Use the exact receiver shown by the chooser; Byakko does not switch to
another granted device.

The relay checks receiver status before each keyboard command. If the paired
keyboard is asleep or offline, press a key on it and retry with Read or reconnect.
If status reports an unread reply from an earlier operation, Byakko rejects the
next logical command. Reconnect the receiver before retrying that uncertain
session; Byakko does not drain, retry or periodically poll feature reports in
the background.

The bounded read exchange is:

1. Send relay status `F7` and read its status reply.
2. Select the paired keyboard with `F6` target `10`.
3. Send one ordinary Nia87 feature request.
4. Poll `F7` for a ready keyboard reply, for at most 20 polls at 100 ms
   intervals. Each status and released-reply read uses the observed 10 ms
   settling interval.
5. Send `FC` to release the held relay reply, then read that operation's
   keyboard response.

The identity request uses the same selected-keyboard exchange once at
the selected device worker's first operation, with the result retained until
explicit reconnect. An ordinary feature read gets its own exchange. Timeouts fail the
operation; there is no automatic retry or idle drain. These bounds apply to
the relay wait, not to an arbitrary firmware response guarantee.

## Writes and evidence

Before each feature write, Byakko saves the cached feature before-image and follows
the established feature pacing. Keymap, macro and settings writes get one
feature readback. Ordinary global-lighting and per-key picture setters finish
on transport acceptance after pacing; they do not trigger an automatic getter.
An explicit later Read supplies readback evidence. A successful HID send or
relay status is not proof that a setting persisted.

Windows physical checks on one paired Nia87 passed feature reads and these
native Iced write tests with the current 20-poll relay bound:

- Base F12 was mapped to F11. A physical F12 press produced F11 press/release
  events in the tester on the receiver's exact `MI_00` input endpoint. Both
  complete keymap layers were then restored byte for byte.
- Debounce changed from 1 to 2 ms and back. The complete settings snapshot
  matched its original bytes after restoration. A separate 2.4 GHz sleep
  setting changed from 2 to 10 minutes and read back as 600 seconds, with the
  other settings unchanged. This does not measure the actual inactivity timer.
- Global brightness changed from 4 to 2 and back, with visible rainbow output
  and an exact final lighting snapshot. Earlier steady red/green checks also
  had visible output; a subsequent comparison found keymaps, settings and the
  captured picture unchanged.
- In per-key layer 1, F12 changed from black to red. The camera showed red at
  F12, and an explicit read found only that picture entry changed. All 384
  picture bytes and their selector context were restored exactly, followed by
  restoration of the original global lighting, including speed and rainbow.
- An unassigned, originally empty macro slot stored B down, a 20 ms wait and
  B up with repeat count 1. Its complete readback matched the program. The
  developer native-backup restoration path restored all 256 original zero
  bytes, including the stored zero count. Playback was not tested.

When the keyboard naturally slept before the F12 restoration, Byakko rejected
the save and retained the original target as a draft. After a physical wake,
explicit Read / reconnect preserved that draft, and its save/readback restored
the complete original maps. This verifies that bounded sleep/wake path, not
arbitrary interrupted-write recovery or power-cycle persistence.

The earlier five-poll RGB check timed out on a held lighting reply; the current
tests close ordinary post-set acceptance with the longer bounded wait. Nia87's
intentional global-white convention remains semantic `FF FF FF` to wire
`FA FF FA`; exact comparisons must still report any raw byte differences.

Browser WASM tests cover a simulated relay only, not physical WebHID on this
receiver. Receiver host-lighting streams remain blocked as unverified. Browser
receiver notification delivery remains disabled. Native input notifications
remain unverified until an actual event is observed. Linux builds and descriptor
checks do not establish physical receiver access or a working udev grant.

See [pre-alpha support](pre-alpha-support.md), the
[acceptance ledger](parity-status.md), and the
[browser transport notes](webhid-browser.md) for the wider product limits.

## Native research clients

`BoundNia87Adapter` binds without I/O and establishes receiver board identity on
its selected executor worker. Low-level clients may instead call the explicit
synchronous `device::Access::connect(target)` once, then reuse that immutable
access. Unbound convenience feature functions require the wired transport;
receiver operations must use selected, board-validated access. Raw HID research
APIs remain available for protocol diagnostics.
