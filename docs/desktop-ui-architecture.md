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

The composition root supplies a backend-specific read-only availability probe
to Iced. A bounded worker performs HID enumeration and reports a backend-neutral
missing, single-device, ambiguous or error state. The desktop owns scan timing,
connection generation changes and user-facing status; core owns drafts and
conflict detection. Each accepted connection creates a serialized executor
with an immutable backend target. Its native opens check that same target,
including readback and recovery, before accessing a HID collection.

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
The reviewed apply action now uses that typed outcome and requires an exact
before-image, durable backup and complete readback. A failed apply retains the
review for inspection but requires a fresh review before retrying. No live
Iced archive apply has been attempted.

This is a pre-alpha composition layer, not a frozen visual design. Toolbars
and compact pane interaction still need rendered UI review. Static interface
copy currently lives near its view; backend labels and control availability
come from descriptors and projections. Future capabilities should use
the same data and control boundaries instead of adding one-off widget trees or
mutable duplicate drafts. Host-driven effects and 2.4 GHz support remain later
capabilities.

## 2026-09-24 grouped key assignment review

The assignment catalog now has typed backend-supplied groups: alphanumeric,
modifiers, navigation, function keys, numpad, media, mouse, system, shortcuts,
and other. The default view shows one group. Search spans all groups and Enter
stages an exact or unique result; `Num 9` is an alias for the numpad key. A
window-local Type a key action captures one supported physical key; Escape and
focus loss cancel. Shortcut selection now uses search and compact matches rather
than an exhaustive dropdown. Assignment remains staged until Apply.

Wide windows place the browser in a right sidebar beside the persistent keyboard;
narrower windows place it below the board. Tiles fit their labels instead of
using full-width bars. Board legends are derived from the selected layer's draft,
so staged, saved and reverted assignments are reflected without another device
read. Hover identifies the original physical key and full assigned action.

Reference review used McMaster-Carr's category/search organization
(https://www.mcmaster.com/) and the previously recorded official-interface
categories in docs/live-evidence.md. No vendor implementation was copied.
Automated coverage includes group/search isolation, typed assignment, cancelling
capture, remapped legends and revert. Final visual layout and physical key output
remain for human review. Stop after this requested iteration.
