# Desktop UI composition

`byakko-core` owns device-neutral descriptors, constraints, drafts and pure
transitions. A future browser frontend can consume its owned commands and
results. `byakko-devices` owns effects, firmware codecs and the serialized
worker. `byakko-desktop` maps Iced messages into core edits and renders state.

Visual variables live in `crates/byakko-desktop/src/panels.rs` as `UiStyle`:
theme, initial window size, spacing, padding, type sizes, field widths, pane
ratios, compact breakpoint and semantic selection colors. Views receive one
style value through `Desktop`; they do not scatter pixel or color literals.
`panels::panel` and `panels::split` compose every main editor from the same
primitives. The split changes from adjacent panes to stacked panes when the
available width falls below the configured breakpoint. The actual default
theme still comes from Iced; selected controls retain Iced's active and hover
styles unless semantic colors are supplied.

Device capability values stay outside widgets. For lighting, the pure
`byakko_core::lighting::controls` projection turns a validated setting into
named effect choices, level ranges and option choices with edit intents. The
desktop's `control_widgets` renders choice and bounded level controls. The
lighting page chooses panel arrangement and dispatches edits; it contains no
firmware effect IDs, color capability cases or report encoding. The memory
demo advertises a different lighting catalog to exercise that separation.

Per-key picture editing follows the same boundary. The core picture editor
owns a baseline and RGB draft keyed by backend-provided key IDs, while the
desktop renders color edits and sends owned picture commands. Nia87 maps those
IDs to physical matrix slots and retains all 128 RGB triples in its revision,
including slots outside the visible key catalog. Its RGB storage is separate
from choosing the global built-in picture effect. The attached USB device has
only had a read-only baseline check for this Iced slice; no live picture write
has been performed.

Scalar settings use an advertised field catalog with toggle or bounded numeric
types and units. The Iced page renders that catalog in the same split panels;
its demo advertises different fields and ranges from Nia87. Core allows one
staged field at a time because the native transaction applies one setting.
The Nia87 adapter retains all four raw 64-byte replies in its revision and
converts sleep minutes to the device's seconds. The attached keyboard has only
had a read-only check through this Iced path.

Native archive capture and review use a bounded opaque byte value in core, with
the backend advertising format ID and maximum size. Iced owns local file
paths and file work, while the Nia87 adapter owns archive parsing and the
two-direction restoration plan. The archive page composes the same panels and
renders backend-supplied section names/counts. It does not inspect vendor
sections. Reviewed state is invalidated by a write or connection change.
Apply remains separate until the native recovery path reports typed outcomes.

This is a pre-alpha composition layer, not a frozen visual design. Toolbars
and compact pane interaction still need rendered UI review. Static interface
copy currently lives near its view; backend labels and control availability
come from descriptors and projections. Future capabilities should use
the same data and control boundaries instead of adding one-off widget trees or
mutable duplicate drafts. Host-driven effects and 2.4 GHz support remain later
capabilities.
