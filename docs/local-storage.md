# Local files

The native GUI uses a stable per-user data directory, independent of the
shortcut's working directory or the executable's installation folder:

| Platform | Data directory |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Byakko`; fallback `%USERPROFILE%\AppData\Local\Byakko` |
| Linux | `$XDG_DATA_HOME/byakko`; fallback `$HOME/.local/share/byakko` |

Environment paths must be absolute and representable as UTF-8 for the GUI's
text-based export fields. Windows drive-rooted and UNC paths are accepted.
If no usable root exists, or
the directory cannot be created, GUI startup reports an error before opening
the keyboard. It does not silently put backups in a temporary directory.

Automatic GUI transaction backups go in the `backups` subdirectory. Default
keymap and full-configuration export paths are `nia87-keymap.json` and
`nia87-configuration.json` under the data directory. These remain editable in
the GUI. Exports and backups create new files; an existing file is not replaced.
Macro import/export continues to use the path entered in its file field.

Macro Studio's **Save Labels** stores all50 slot names in
`macro-labels/nia87/label-NNNNNNNNNNNNNNNNNNNN.json`. These are local per-slot
preferences shared by Nia87 keyboards on this computer, not names read from
device memory or proof of an individual keyboard's identity. Names load on
the next GUI start. Saving labels does not apply a macro, change a binding,
or persist its play-mode selection. Macro JSON exports still include their name
and selected play mode.

Label saves create numbered snapshots and leave older files intact. The newest
snapshot must validate before loading; a damaged newest file produces a visible
error instead of silently showing older labels. Names are limited to256 UTF-8
bytes and each snapshot to32 KiB. Unsaved name changes stay visibly marked.

Existing files from older builds stay where they were created. Byakko does not
move or delete them; an old archive can still be opened by its explicit path.
CLI commands and research examples retain their explicit path arguments and
documented backup locations. No cloud account or network storage is involved.
