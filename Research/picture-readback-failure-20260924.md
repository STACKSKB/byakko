# Per-key readback failure investigation — 2026-09-24

## What the reported failure establishes

The reported transaction used backup `picture-before-1790221895959179900.json`
(effect 13, option 0). It passed pre-write baseline/selector checks, sent the
changed-slot setters, obtained a stable picture that differed from the desired
128 RGB entries, then restored and verified the original picture. This is a
post-write comparison failure, not a preflight conflict. Verified recovery means
the picture read matched the old colors at that selector; it is not physical LED
or power-cycle acceptance.

The old backup stored only original colors and context. The desired and failed
readback were discarded, so the exact failing slot and returned value cannot be
reconstructed. The three preceding backup transitions show accepted color
changes at slots 51 and 61 under the same selector; this does not establish the
key/color requested in the failing transaction. There is no evidence here that
the user caused a conflict or another configurator interfered.

Plausible explanations remain an ignored/late single-key setter or a response
that does not represent writable RGB storage at that moment. Selector-dependent
reads are independently documented in `picture-selector-audit.md`. A selector
change would normally yield its own error; this incident's saved selector was
13/0. Do not claim any hypothesis is the proven cause or silently call an
unverified setter a successful save.

## Read traffic audit

Byakko does not continuously read pictures while idle. A successful nonempty
picture transaction currently performs at least:

| Stage | `read_payload` exchanges |
| --- | ---: |
| Two identity/keymap snapshots (66 each) | 132 |
| Two pre-write pictures and one post-write picture (24 each) | 72 |
| Three selector checks (4 each) | 12 |
| Total | 216 |

Each exchange sleeps 30 ms: 6.48 seconds of fixed read waits, plus 100 ms per
changed-slot setter and filesystem/enumeration/HID overhead. This excludes
additional stability attempts, failed-write recovery and connection reads.
The last four backup timestamps were about 7.6 seconds apart, consistent with
this expense but not a trace of individual requests. The full keymap reads are
particularly disproportionate to a color edit. This audit does not change pacing
or remove verification without Nia87 evidence.

## Diagnostic correction

Before-write backups now also retain the desired colors. A mismatch creates a
separate `.failure.json` containing original, desired, observed, selector and
changed-slot data before recovery. Errors distinguish edited-slot mismatches
from changes to untouched slots and include bounded examples. Evidence-file
failure cannot bypass recovery. Existing files are never overwritten.

No hardware writes were performed during this investigation. The diagnostic
change is tested without hardware; the underlying intermittent mismatch remains
unresolved pending an observed failing response.

## Other configurators (source reviewed 2026-09-24)

- [Sharkfin per-key writer](https://github.com/dniminenn/sharkfin/blob/master/app/src-tauri/src/commands.rs#L860-L919): bulk picture upload, no subsequent picture comparison; its comment says the getter does not reflect that upload. It retains the pattern on the host. The same file uses paced uploads and cached connection liveness. This differs from Byakko's single-slot opcode0x14 path and is not proof that Nia87's readback is invalid.
- [AM Configurator](https://github.com/roethlar/AMKB-GUI#before-you-write-to-a-keyboard): documents keymap/macro readback after full configuration writes, with visual lighting checks where firmware cannot report lighting. Its [reader](https://github.com/roethlar/AMKB-GUI/blob/main/am_configurator/reader.py) documents the absence of a known LED-frame readback path. Different devices/protocol; no claim about every configurator or Nia87.
- The official Nia87 bundle audit in `picture-selector-audit.md` establishes bulk picture commands and the simple setter, but the extracted bundle is not present in this checkout. That audit does not establish a continuous-readback policy. Do not invent a conclusion for the official UI.

External implementation source was read for protocol/workflow observations only;
no vendor or copyleft code was incorporated. Links above track mutable branches.

Validation: 188 devices tests, 76 desktop tests, 68 core tests and 15 CLI tests
passed; devices/desktop all-target Clippy passed with warnings denied. The release
build is `target/ux-review/release/byakko-desktop.exe`.
