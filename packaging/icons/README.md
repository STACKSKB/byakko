# Byakko icon

An original geometric sans-serif **B**, inspired by the proportions of Futura.
No font files or character artwork are used. GPL-3.0-or-later.

Regenerate SVG, PNG, ICO and the 64×64 window RGBA pixels with
`python tools/generate_icon.py` (requires Pillow). From this directory, regenerate
the Windows executable resource with the Windows SDK's
`rc.exe /nologo /fo byakko.res byakko.rc`.
Generated assets are committed so ordinary application builds need neither
Python nor an extra resource compiler invocation.
