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

The 2026-09-27 follow-up in layer.rs accounts for cached text alignment when
calculating damage. Pick-list labels use a vertical-center anchor; treating it
as the top-left left the upper half of the old label visible after replacement.
Damage now includes the aligned text rectangle. The regression replaces an
effect name with the opaque-state placeholder and back, comparing incremental
pixels with a fresh frame at 100%, 125% and 200% scale. It fails against the
original damage calculation. This preserves partial redraws and the existing UX.

The 2026-10-11 follow-up expands the old and new clip bounds by one logical
pixel when a layer's bounds change. Antialiased bordered quads can leave pixels
just outside their geometric clip. A focused regression moves a bordered quad
with its clip, as happens when the keyboard moves between tester and
configurator scroll layouts, and compares incremental and full renders at 100%
and 125% scale. It fails with the old exact-bounds calculation and passes with
the one-pixel expansion. The ordinary changed-quad path already used that
expansion. The regression isolates the renderer behavior; native window
transition acceptance remains a separate check.

Further 2026-10-11 regressions cover related partial-repaint boundaries.
Changed quads now retain a one-pixel fringe outside an unchanged layer clip
when calculating damage; removing a bordered key from that clip previously
left 82 pixels from the old key at 100% scale. Quad drawing now applies the
clip mask whenever antialiasing or a stroke may extend beyond the geometric
quad, so a full frame and a narrower damage replay paint the same edge. That
partial-versus-full regression previously differed at 202 pixels. Finally,
physical damage is rounded outward to whole pixels before clearing and
replaying layers; a fractional damage region previously left 102 stale pixels
in its pixel envelope. These focused cases pass at 100% and 125% scale. The
full native page-transition comparison and live-window acceptance are separate
checks; these tests alone do not claim every transition is pixel-identical.
