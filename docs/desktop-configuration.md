# Desktop autosave configuration

`BYAKKO_AUTO_SAVE_DELAY_MS` configures the idle gap before queued lighting and
settings edits are sent. The default is **350 milliseconds**; valid values are
0–10000. Set it in the environment before launching Byakko. Zero requests an
immediate send whenever the serialized device worker is available.

For example, in PowerShell:

```powershell
$env:BYAKKO_AUTO_SAVE_DELAY_MS = '500'
& .\byakko-desktop.exe
```

The composition root supplies `config::Config` to the desktop. The widget code
does not read the environment or choose a hardcoded save interval. Tests can
supply a deterministic delay directly.

Per-key lighting retains the brush color. Clicking another key paints that key;
each paint restarts the idle deadline. The on-screen board changes immediately,
then one complete picture is uploaded after the gap. Edits during an upload
are retained for the next batch. Onboard lighting coalesces parameter changes
in the same way. Settings retain the latest value per field and send native
one-field transactions sequentially while the controls remain editable.

Sleep sliders use their backend-advertised range, with **Disabled** as the final
stop after the maximum. Disabled maps to the backend's zero sentinel; it is not
an additional firmware timeout value.
