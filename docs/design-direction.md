# Native workbench design

The user requests an original interface informed by McMaster-Carr's information density, FL Studio's editing workspaces, and Vim's keyboard-oriented operation. Neither the OEM application's UX nor Sharkfin's UX is a design template. Those applications are behavioral and protocol references only.

## Interaction principles

- Open directly onto the detected Nia87 and its built-in physical layout. Keep connection, layer, selected key, and pending changes visible.
- Favor compact, labeled controls, searchable action catalogs, numeric fields, and editable event tables over decorative panels or setup wizards.
- Separate the physical keyboard from its assignments: selecting a key inspects its current action without changing the layout.
- Make keyboard navigation useful through visible shortcuts, predictable focus, search, and explicit apply/revert actions. Vim inspiration does not require a modal editor or hidden commands.
- Treat macros as event sequences with precise delays, ordering, repeat settings, and a clear storage budget. Preserve unfamiliar actions rather than silently normalizing them.
- Show device state, local drafts, and verified saved state distinctly. Applying changes performs a backup and readback; status must describe evidence, not imply physical playback verification.
- Keep a restrained paper/ink palette with one accent, strong alignment, legible labels, and useful density. Color supplements text and selection outlines.

## Implementation boundaries

Rust with native Iced rendering; no JavaScript, Electron, embedded browser, or vendor helper in the desktop product or its build. The retained egui application is a research baseline, not the product UI. Research copies of the supplied official application remain outside distributable source. No vendor or Sharkfin artwork, layout assets, source components, or interaction flows are incorporated.

## Review criteria

The first-use screen should answer: which keyboard is connected, which layer is being edited, what is assigned to the selected key, and what will change on Apply. Macro editing should be usable without opening another application. Narrow windows must retain access to all controls through intentional scrolling. Native keyboard accessibility and Linux behavior still need hands-on verification.
