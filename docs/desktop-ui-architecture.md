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

This is a pre-alpha composition layer, not a frozen visual design. Toolbars
and compact pane interaction still need rendered UI review. Static interface
copy currently lives near its view; backend labels and control availability
come from descriptors and projections. Future settings and per-key editing
should use the same data and control boundaries instead of adding one-off
widget trees or mutable duplicate drafts.
