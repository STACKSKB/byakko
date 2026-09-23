# Nia87 picture selectors and color storage

Read-only selected-path audit, 2026-09-22. Source is the user-supplied/current
ignored `Research/extracted/web-current/main_68eaf5ce.js`. Offsets below are
character positions in that exact minified bundle. Only interface and protocol
observations are recorded here; no implementation code is incorporated.

The Nia87 registration selects layout `oc` and the `Emt`/`YHe` inheritance path.
Its `LightUserPicture` catalog has three option labels, 1/2/3, near 9,543,394.
These labels alone do not establish three independent color banks.

| Selected path | Observation |
| --- | --- |
| `$Y.setLightPic`, 15,428,784 | Maps RGB colors by default-matrix index and dispatches to the selected lower-level writer. Its optional second argument does not establish bank support in that writer. |
| `AC._setLightPic`, 15,447,350 | Sends seven 56-byte pages with header index byte1 fixed at zero. The optional upper-level argument is not used to select a bank. |
| `$Y.getLightPic`, 15,430,027 and `AC._getLightPic`, 15,448,055 | Reads six pages, again with request index byte1 fixed at zero. |
| Nia87 general picture store, 23,575,886 and 23,576,402 | Calls the bulk set/get operations without a bank argument. |
| `YHe.setLightPicSimple`, 17,782,460 | Single-key opcode0x14 takes an optional second argument in byte1, matrix index in byte2, RGB in bytes8–10. No selected UI caller establishes a bank meaning for that argument. |
| Separate family UI, 25,147,657 | Passes a bank selection, but its store is gated by `keyboard3123` at 24,184,450; this is not evidence of Nia87 support. |

The selected path exposes one *addressed* color store and three global effect
options. Byakko's effect13 setter puts option indices0/1/2 in the high nibble
of global-lighting byte4. The bundle alone does not establish whether the
firmware internally switches color banks when this option changes.

## Selector-only live comparison, 2026-09-23

The attached board began at effect1, brightness4 and fixed RGB `(8,8,8)`.
A complete archive and index0 picture read were saved before any setter.
Guarded global-lighting transactions selected effect13 option1 and then
option2. **No per-key color setter was sent.** Option1's index0 384-byte
picture response matched the original exactly. Option2's response differed
at 15 RGB entries: 11 formerly black positions read red and four formerly
red positions read black. The webcam also showed visibly different red-key
patterns in option1 and option2. The official index0 reader therefore returns
different colors after a global option change on this firmware.

The original effect1 setting was restored from a fresh revision. A complete
archive afterward compared equal to the before archive (`[]`). This proves
restoration of the archive's observable state, not the persistence or meaning
of every possible hidden picture store. Option3 was not selected during this
interrupted comparison. The result warrants a bank/selector model before
claiming full per-key picture parity; it does not by itself prove three
independently writable banks. Local ignored evidence files use prefix
`Research/captures/picture-options-` and webcam stills
`keyboard-camera-1790161518232179300.png` (option1) and
`keyboard-camera-1790161630117796700.png` (option2).
