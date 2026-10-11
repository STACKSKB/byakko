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
keyboard was validated once at connection as board `0x095f`, firmware `0x0100`,
profile 0. Do not infer pairing or write support from the receiver VID/PID
alone. Use the exact receiver shown by the chooser; Byakko does not switch to
another granted device.

The relay checks receiver status before each keyboard command. If the paired
keyboard is asleep or offline, press a key on it and retry with Read or reconnect.
If status reports an unread reply from an earlier operation, Byakko rejects the
next logical command. Unplug and reconnect the receiver to clear that uncertain
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
connection. An ordinary feature read gets its own exchange. Timeouts fail the
operation; there is no automatic retry or idle drain. These bounds apply to
the relay wait, not to an arbitrary firmware response guarantee.

## Writes and evidence

Before each setter, Byakko saves the cached feature before-image and follows
the established feature pacing. Keymap, macro and settings writes get one
feature readback. Ordinary global-lighting and per-key picture setters finish
on transport acceptance after pacing; they do not trigger an automatic getter.
An explicit later Read supplies readback evidence. A successful HID send or
relay status is not proof that a setting persisted.

Windows physical checks on one paired Nia87 have passed reads of keymaps,
settings, global lighting, picture context and macro slot 49. In a user-approved
RGB check, red and green were visible and explicit reads passed. A rainbow
lighting setter was accepted, but its verification reply exceeded the earlier
five-poll (500 ms) wait. The operation's own held `0x87` lighting reply was
later retrieved with `FC` and showed the original setting restored. The only
reported byte difference was the existing semantic-white convention: semantic
`FF FF FF` is represented on the wire as `FA FF FA` at offsets 5 and 7. This
does not establish that other features were unchanged: the subsequent
cross-feature comparison could not run because the paired keyboard was asleep.

The relay wait is now bounded at 20 polls (about two seconds); longer-window
physical post-set acceptance remains pending. Browser WASM tests cover a
simulated relay only, not physical WebHID on this receiver. Receiver host-lighting
streams remain blocked as unverified. Browser receiver notification delivery remains disabled. Native input notifications remain unverified until an actual event is observed. Linux builds and descriptor
checks do not establish physical receiver access or a working udev grant.

See [pre-alpha support](pre-alpha-support.md), the
[acceptance ledger](parity-status.md), and the
[browser transport notes](webhid-browser.md) for the wider product limits.
