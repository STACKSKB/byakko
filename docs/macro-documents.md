# Macro documents

In Iced, read a slot and open **Macro file…**. Enter a JSON file path and choose
**Import draft** or **Export new file**. Import replaces only the staged program;
Revert restores the device baseline. Saving to the keyboard remains a separate
explicit action. Export includes the current draft, including unsaved edits.

Inputs and outputs are limited to 64 KiB. Invalid JSON, unsupported versions
and programs outside the target's capabilities leave the existing draft
unchanged. Export serializes before creating the destination, uses exclusive
creation, and refuses an existing file. An I/O failure is reported; it does not
establish a successful export or change device trust.

## Formats

| Format | Import | Export |
| --- | --- | --- |
| Native v1, produced by the retained research app | Converts original slot/name/play mode and native events | Retained research app only |
| Device-neutral v2 | Validates the program against the selected target | Iced default |

Version 2 contains `format_version`, `backend_id`, `source_slot`, `name`,
optional `binding`, and a typed `program` with repeat count, actions and wait-after
values. The source identity is descriptive. It never switches the selected slot
or chooses another device. Standard key/button usages may transfer where target
capabilities permit; backend-local actions must retain compatible identifiers.
This is not a promise that all macro programs transfer between firmware families.

`binding` is an advisory backend-local preference. Import does not bind a key,
change playback mode or silently adjust repeat count. A preference from another
backend or absent from the target slot is discarded with a notice; a compatible
one is retained for export. Successfully staging a binding updates that file
preference. Nia87 toggle/hold continue to require explicitly saved count 1.

The macro name field and binding preference are per-slot session metadata,
preserved in exported documents. They are not keyboard storage and are not yet
automatically persisted across app restarts. Importing a document does not add
it to the action catalog or send HID requests.

The document DTO lives in core. Bounded reading, legacy conversion and exclusive
file creation live in devices. Iced supplies paths and dispatches file work
asynchronously. The core file ticket excludes concurrent editing/device work
and rejects stale completions. Future browser adapters can supply decoded
document values without making core depend on a native filesystem.
