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

Existing files from older builds stay where they were created. Byakko does not
move or delete them; an old archive can still be opened by its explicit path.
CLI commands and research examples retain their explicit path arguments and
documented backup locations. No cloud account or network storage is involved.
