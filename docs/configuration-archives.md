# Local configuration archives

`capture-configuration NEW_PATH.json` reads the connected Nia87's saved state:
both 128-entry keymaps, all 50 raw macro slots, global lighting, the current
128-color picture, and the four settings replies. The native Keys page offers
the same operation under **Configuration archive**. Unsaved editor drafts and
host audio/screen streams are not part of the archive.

Capture holds the Byakko transaction lock and one HID handle throughout. Each
section uses its existing repeated-read checks, and two complete captures must
match before a file is created. Close the official helper and other
configurators first: they do not honor Byakko's lock. Progress counts macro slots
across two passes (100 total). Capture can take a few minutes; it sends read
requests only. Device disconnection or inconsistent responses abort the export.

Files use the original `byakko-configuration` JSON format, version1, limited to
Nia87 firmware0100/profile0. Import checks fixed sizes and identity and is
limited to 256KiB. Raw macro bytes remain intact even when their actions are
unknown to the editor. Files are created exclusively; an existing file is never
overwritten. `inspect-configuration PATH.json` validates and reports section
counts without opening the keyboard or printing macro contents.

Use **REVIEW RESTORE** to compare an archive with a fresh complete capture.
The review lists changed sections; **APPLY REVIEWED ARCHIVE** is a separate
action. Apply rechecks the complete expected state and saves a durable archive
before sending setters. Macro contents are written before their bindings.
Reserved slots, opaque setting differences and changed macros that cannot
round-trip exactly are rejected before writes. Unknown unchanged bytes remain
preserved. Saved host lighting modes do not automatically start screen/audio
capture. Other editor drafts are retained and those panels must reload afterward.

On failure, recovery first restores keymaps, then attempts each affected macro,
picture, scalar setting and lighting section. An individual section failure
does not suppress later recovery attempts. Complete repeated readback determines
whether recovery succeeded. Disconnection, process termination and partial-write
failure recovery still need fault-injection acceptance tests; the durable
before-image is retained regardless. Normal window close is held while archive
application is active. The existing keymap and macro import/export remain
available separately.

`plan-configuration CURRENT.json TARGET.json` checks both directions and prints
change counts without device access. It does not establish that CURRENT still
matches the keyboard; Apply always rechecks that itself.

The current picture is captured; this does not claim that all three advertised
picture banks have been independently identified.

On the attached Windows Nia87, the first live capture completed both full passes
and saved a 139,657-byte archive. Inspection found 50 macro slots (one nonempty),
128 bindings per layer, 128 colors and effect5. The keymaps and all 256 slot0
bytes matched the preceding verified backups. A concurrent `inspect` command
was rejected while capture held the lock. The private fixture remains ignored
at `Research/captures/configuration-first-complete.json`.

The reversible `verify_configuration_roundtrip` example then changed Pause to
F24, populated unbound macro49, changed picture slot91, reduced Ripple brightness
from4 to3 and changed debounce from1 to2 in one archive application. It applied
the original archive afterward. Both applications passed complete repeated
readback of every archive section. No key was physically pressed and the macro
was never bound or played. This normal round trip does not test a failed setter
or unplugged-device recovery. The first injected post-delivery error test failed;
unexpected macro, picture and lighting differences were subsequently restored
from the durable original archive and fully verified. See the
[fault investigation](../Research/configuration-fault-verification.md).
Automatic failure recovery is not yet accepted.

Archive files may contain personal keyboard macros. They stay at the chosen
local path; Byakko performs no upload or sharing.
