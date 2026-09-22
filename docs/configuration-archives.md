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

This first archive implementation supports capture and inspection. Applying a
complete archive is still pending: it needs supported-field validation,
expected-state checks, a durable before-image, ordering of macros before their
bindings, and verified recovery across section failures. The existing local
keymap draft and individual macro import/export continue to work separately.
The current picture is captured; this does not claim that all three advertised
picture banks have been independently identified.

On the attached Windows Nia87, the first live capture completed both full passes
and saved a 139,657-byte archive. Inspection found 50 macro slots (one nonempty),
128 bindings per layer, 128 colors and effect5. The keymaps and all 256 slot0
bytes matched the preceding verified backups. A concurrent `inspect` command
was rejected while capture held the lock. The private fixture remains ignored
at `Research/captures/configuration-first-complete.json`.

Archive files may contain personal keyboard macros. They stay at the chosen
local path; Byakko performs no upload or sharing.
