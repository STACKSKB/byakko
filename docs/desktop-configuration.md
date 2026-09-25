# Desktop autosave configuration

`BYAKKO_AUTO_SAVE_DELAY_MS` configures the idle gap before queued color and
settings edits are sent. The default is **2000 milliseconds**; valid values are
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
are retained for the next batch. Onboard color edits coalesce in the same way. Lighting mode, brightness,
speed and option changes send immediately when the device worker is available. Settings retain the latest value per field and send native
one-field transactions sequentially while the controls remain editable.

Sleep sliders use their backend-advertised range, with **Disabled** as the final
stop after the maximum. Disabled maps to the backend's zero sentinel; it is not
an additional firmware timeout value.

Moving within the color picker restarts the lighting idle deadline, even when
the RGB value is unchanged. Holding a picker drag suspends lighting sends until
release, then starts a fresh idle gap.
