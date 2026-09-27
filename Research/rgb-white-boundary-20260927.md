# RGB white boundary — Windows Nia87, 2026-09-27

The user authorized literal RGB boundary tests and comparison with the official
software. On this unit, the evidence rejects a general stored-value cap of 250
on red or blue. It instead supports special handling of the exact all-255 tuple.
This tests stored protocol values, not optical intensity or internal LED PWM.

Following this comparison, the user directed that Byakko's Nia87 backend mirror
the official white behavior and treat it as intentional, not a bug. The required
global-lighting contract is semantic `FF FF FF` → wire `FA FF FA` → semantic
`FF FF FF`, with lossless raw snapshots. This supersedes the earlier unresolved
white-bug/gate classification; unrelated recovery failures remain separate.

## Native literal writes

The ignored `target/rgb-acceptance` harness opens one validated, immutable Nia87
collection and holds the configuration lock and HID handle throughout. It saves
and syncs the complete lighting baseline before sending any setter. Reports use
the existing `0x07` layout and BIT8 checksum, Always On, brightness 4 and custom
RGB (`07 01 04 04 07 R G B checksum`). Literal RGB bytes bypass the normal
semantic white-to-near-white encoder. The official helper was stopped first.

Each setter has the established 500 ms pacing plus a two-second diagnostic wait,
then a getter; a second getter follows two seconds later. Both reads agreed in
all 18 cases. Expected value differences do not trigger an automatic rollback
between samples. The final restoration uses the same handle and saved baseline.

| Literal RGB written | First and second stored RGB |
| --- | --- |
| 253,253,253 | 253,253,253 |
| 254,254,254 | 254,254,254 |
| 255,255,255 | **180,180,180** |
| 255,250,250 | 255,250,250 |
| 250,255,250 | 250,255,250 |
| 250,250,255 | 250,250,255 |
| 250,255,255 | 250,255,255 |
| 255,250,255 | 255,250,255 |
| 255,255,250 | 255,255,250 |
| All six permutations of 255,254,253 | Each permutation preserved exactly |
| 255,0,0; 0,255,0; 0,0,255 | Each primary preserved exactly |

The earlier exact archive restoration attempt also read back `180,180,180`
after literal all-255 under Wave/rainbow (`0x04`, flags `0x08`); its mismatch
artifact was re-examined in this follow-up. Thus the previous final
`250,255,250` was the recovery target, not the immediate result of that literal
all-255 write. The distinction matters when interpreting that experiment.

Native evidence: `Research/captures/rgb-boundary-20260927/native/trace.jsonl`,
SHA-256 `b48840df3660e448ccb2e5609456ef8cc99efa06b41976262e48769a8669aab4`.
It records exact outgoing host buffers, transport outcomes, both raw replies and
restoration. This is an API-level trace, not a USB bus capture or firmware source
inspection. The all-255 conversion occurs without the vendor application/helper
or Byakko's semantic conversion in the path, strongly supporting a device-side
special case; it does not identify the internal firmware mechanism.

## Official software comparison

Windows native screenshot capture still failed with `SetIsBorderRequired failed:
No such interface supported (0x80004002)`. After separate explicit user approval,
the existing official Nia87 Driver 2.1.97 renderer ran in the Codex browser against
its original local `iot_driver_v200` helper. This is the vendor's browser branch,
not an unmodified Electron-window test.

A local research server supplies two display-only Electron methods (scale factor
1 and a no-op zoom) so the original renderer can mount. It redirects the existing
helper URL from port 3814 through the local byte-recording proxy at 3815. The
device codec, feature handlers and helper binary are unchanged. The original
installation files and product source are unchanged. Browser mode displays
ROYUAN branding and lacks some decorative assets; it detects the Nia87 and uses
the vendor's original lighting controls and local helper.

The committed custom Hex field was used for nine samples. The embedded color
picker's separate Hex field only changed local preview in the first attempt;
no write was inferred from that preview. Each committed sample was followed by
a page reload to obtain the vendor's fresh `0x87` response, then inspection of
the RGB controls. This diagnostic repetition is not a proposed runtime policy.

| Entered official Hex | RGB in outgoing helper request | Raw RGB on subsequent read | Reloaded UI |
| --- | --- | --- | --- |
| FDFDFD | 253,253,253 | 253,253,253 | FDFDFD |
| FEFEFE | 254,254,254 | 254,254,254 | FEFEFE |
| FFFFFF | **250,255,250** | **250,255,250** | FFFFFF |
| FFFAFA | 255,250,250 | 255,250,250 | FFFAFA |
| FAFAFF | 250,250,255 | 250,250,255 | FAFAFF |
| FAFFFF | 250,255,255 | 250,255,255 | FAFFFF |
| FFFAFF | 255,250,255 | 255,250,255 | FFFAFF |
| FFFFFA | 255,255,250 | 255,255,250 | FFFFFA |
| FAFFFA | 250,255,250 | 250,255,250 | **FFFFFF** |

The software itself substitutes `250,255,250` before asking the helper to send
white. It also interprets that exact tuple as white when reading. This explains
why its apparent white roundtrip succeeds while a literal all-255 raw restore
does not. Byakko's existing semantic encoder/decoder follows that convention.
The alternate channel arrangements and nearby greys are not capped.

Local RPC evidence and decoder are under
`Research/captures/rgb-boundary-20260927/`. The research decoder handles the
browser's bodyless CORS OPTIONS requests before using the existing protobuf
decoder. Original streams are retained under that directory's `Research/captures`.
The capture is at the renderer/helper boundary before report-ID/checksum framing;
it is not a new final-HID debugger capture. Connection timestamps identify
connection creation, not individual request times; chronological UI test order
is recorded above separately from per-connection exchange indices.

## Restoration and scope

Both the native sweep and official UI restoration returned to the starting
Wave/rainbow state: effect 4, speed 2, brightness 4, right, raw RGB 250,255,250.
After closing the browser and stopping the helper, an independent complete
archive sweep matched the pre-test archive byte for byte: keymaps, all fifty
macro slots, lighting, captured picture and settings. `before.json`,
`after-native.json` and `after-official.json` all have SHA-256
`ca4cc4ca7d7f2b3b920bb488da57a91ad074784aeab12ce20a67d4a82dedc1e1`.
The temporary servers, recorder and helper were stopped.

This restores this experiment's canonicalized starting state. It does not claim
to restore the earlier all-255 raw archive. Its exact-byte limitation is expected
under the intentional Nia87 white convention, not an outstanding white bug.
No production code, firmware, macro assignment, power-cycle or fault experiment
was changed/performed. A later optical comparison would need controlled exposure
or measurement; these near-white readbacks do not establish emitted brightness.
