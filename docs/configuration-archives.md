# Local configuration archives

`capture-configuration NEW_PATH.json` reads the connected Nia87's saved state:
both 128-entry keymaps, all 50 raw macro slots, global lighting, the current
128-color picture, and the four settings replies. The native Keys page offers
the same operation under **Configuration archive**. Unsaved editor drafts and
host audio/screen streams are not part of the archive.

Capture reads each advertised feature once in one complete sweep and holds one
HID handle throughout. Macro progress covers the 50 advertised slots. Capture
sends read requests only; device disconnection or an invalid response aborts
the export.

Files use the original `byakko-configuration` JSON format, version1, limited to
Nia87 firmware0100/profile0. Import checks fixed sizes and identity and is
limited to 256KiB. Raw macro bytes remain intact even when their actions are
unknown to the editor. Files are created exclusively; an existing file is never
overwritten. `inspect-configuration PATH.json` validates and reports section
counts without opening the keyboard or printing macro contents.

The public Iced page supports capture and export only. Whole-configuration
review and restore remain developer research APIs and are not part of the
public pre-alpha workflow. The historical research record below does not
establish restore acceptance; its failure-recovery discrepancy remains open.

The retained developer restore APIs keep typed recovery results and their
research acceptance limits. Automatic failure recovery remains unaccepted;
the before-image and failure evidence below are retained.

The native capture controller owns an explicit idle/capturing/captured/exporting
state. Changing the export path or starting a device read invalidates a completed
capture. These transitions are tested without device access.

`plan-configuration CURRENT.json TARGET.json` checks both directions and prints
change counts without device access. It does not establish that CURRENT still
matches the keyboard; Apply always rechecks that itself.

The addressed index0 picture is captured. The official Nia87 path exposes three
global picture-effect options, but its selected bulk color reader/writer address
index0; these are not established as three editable banks. See the
[selected-path audit](../Research/picture-selector-audit.md). The archive does
not claim to cover any undiscovered firmware banks.

Historical capture evidence: on the attached Windows Nia87, the first live
capture saved a 139,657-byte archive. Inspection found 50 macro slots (one nonempty),
128 bindings per layer, 128 colors and effect5. The keymaps and all 256 slot0
bytes matched the preceding verified backups. A concurrent `inspect` command
was rejected while capture held the lock. The private fixture remains ignored
at `Research/captures/configuration-first-complete.json`.

Historical developer research: the reversible `verify_configuration_roundtrip` example changed Pause to
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
