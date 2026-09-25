# iced_tiny_skia cached-text clipping patch

Vendored from crates.io iced_tiny_skia 0.14.1 (Iced commit
0ecf60664df7b8ac7d7aef5f7279d5323027f693, tiny_skia directory).
MIT licensed; upstream LICENSE included. Only Cargo.toml and src are taken
from the published crate. This is toolkit dependency code, not configurator
reference application code.

The Cached branch in engine.rs used the text clip rectangle as if it were
painted glyph bounds. It skipped masking when that rectangle fitted inside
the damage region, allowing clipped menu rows to draw over the closed picker.
The local patch always masks cached text to the intersection of its declared
clip and the current layer/damage bounds. No timing or full-window redraw
workaround is involved. Remove this patch when an upstream release includes
an equivalent fix and the raster regression passes against it.
