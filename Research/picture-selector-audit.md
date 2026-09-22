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

The current Nia87 interface therefore exposes one addressed color store and
three global effect options. Byakko's effect13 setter puts option indices0/1/2
in the high nibble of global-lighting byte4. The existing index0 per-key editor
and global option selector reflect the established interface. Do not add three
editable banks or claim they are factory presets based solely on the labels.

This does not prove the firmware lacks other banks, nor whether selecting an
option changes what index0 returns internally. A selector-only test that reads
the same colors in all three modes would be inconclusive: every read still
addresses index0. Distinct results would justify further investigation. No
hardware writes were performed for this audit, and physical option behavior
remains unverified. Native simple per-key writes already have independent
stored-byte readback/restoration evidence; this audit does not replace it.
