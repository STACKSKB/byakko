# Embedded font notices

Byakko's default egui fonts come from the locked crates.io package
`epaint_default_fonts` 0.36.2. The font files are embedded by that dependency;
this directory contains its supplied notices, copied without modification.

| Font | Attribution | Supplied notice |
| --- | --- | --- |
| Hack-Regular.ttf | Copyright 2018 Source Foundry Authors; Copyright 2003 Bitstream Inc.; DejaVu contributions stated as public domain | [Hack-Regular.txt](Hack-Regular.txt), including MIT and Bitstream Vera terms |
| NotoEmoji-Regular.ttf | Copyright 2013 Google Inc. All Rights Reserved | [OFL.txt](OFL.txt) |
| Ubuntu-Light.ttf | Copyright 2011 Canonical Ltd. | [UFL.txt](UFL.txt) |
| emoji-icon-font.ttf | Copyright (c) 2014 John Slegers | [emoji-icon-font-mit-license.txt](emoji-icon-font-mit-license.txt) |

The Noto and Ubuntu copyright lines above are transcribed from each font's
embedded name-table copyright record; the supplied generic license files do
not contain those lines. No font files have been modified or added to this repo.

`source-hashes.json` records SHA-256 for the four source fonts and four source
notices from the locked package. These notices cover font assets only, not all
dependencies or the application itself. Release packaging must include this
directory alongside the remaining dependency notices, which are still pending.
