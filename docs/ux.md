# Browser visual direction

This reference guides the optional browser client's visual styling. It does not replace the approved screenshots and interaction flows or authorize changes to feature placement, device behavior, or transport policy. Keep the persistent keyboard workspace and existing feature navigation recognizable while refining visual hierarchy and surface treatment.

## Direction

The standalone configurator opens with the StacksKB light palette, a black header,
white controls and a neutral work surface. Saved appearance settings take precedence.
The full physical keyboard occupies its own stage. Keep its size and position stable
as the user changes features, selects a key or edits a draft. On compact screens
place the independently scrolling editor below the keyboard and provide a key chooser.

## Appearance preferences

The browser's Appearance dialog offers System, Light and Dark themes, plus
Indigo, Slate, Forest, StacksKB and Custom palettes. System follows the operating system's colour
preference, including changes during a visit. The choice is stored locally in
the browser, separately from keyboard settings and backups. If storage is
unavailable, the choice still applies to the current visit.

The StacksKB light preset uses the [StacksKB](https://stackskb.com/) interface's
neutral canvas and surfaces, black buttons and selected controls, and blue focus
colour. Its dark preset is a Byakko adaptation, not an official StacksKB theme.
Custom provides separate saved light and dark colours for canvas, surface, text,
muted text, accent, focus and rules. Save applies only combinations with readable
text and visible accent/focus contrast.

All interface colours belong to semantic custom properties in
[palette.css](../web/palette.css); [style.css](../web/style.css) owns layout and
uses those properties. Define both light and dark values when adding a palette.
Keep text, focus, selected, disabled, error and modal surfaces legible in every
combination. Physical keycap colours and actual RGB swatches remain independent
of the interface palette. The small [appearance module](../web/appearance.mjs)
owns the preference and dialog without dispatching device commands.

## WordPress embed

Keep the standalone visual direction above. The WordPress embed uses white
surfaces, neutral ink, an indigo accent and the parent page's font family. It
presents a plain **Keyboard configurator** heading within the site's existing
header and footer. Physical keycap colours stay independent of the interface
palette.

The page owns scrolling. Size the iframe to its intrinsic content, remove app
viewport-based minimum heights in embedded mode, and keep dialogs inside the
visible outer-page region. Fit the physical keyboard to narrow screens and keep macro lists bounded. Place the standalone link below the
embed. WordPress appearance defaults to Light/Neutral and uses a separate
storage key; explicit light/dark and alternate palette choices remain available.
The Neutral palette option is shown only in WordPress.

## Keyboard illustration

The stock Nia87 illustration shows a light case, pale alphanumeric keys and lavender
modifier, function and navigation groups. Its colors are independent of the app theme.
Display the verified model as the device name. The keyboard does not report its switch
or keycap finish.

Keep physical legends visible while exposing assigned actions in the inspector. Lighting
keeps all advertised firmware modes. On a loaded per-key layer, choosing a Look previews
a static color map locally. Apply uses the existing picture transaction. A preview
preserves hand-painted keys by default. Replace painted keys explicitly includes those
positions. The gallery tiles show a color preview and the Look name.

## Layout principles

Use direct labels, compact product controls, narrow category navigation and
hairline dividers to make dense settings easy to scan. Keep the keyboard visible
as the focus of the workspace. Do not copy third-party logos, photography, text
or distinctive branded assets.

## Avoid

- Gradients, glow effects, oversized corner radii, and ornamental shadows.
- Repeated pills or nested stacks of bordered cards.
- Large marketing copy or decorative elements that compete with the keyboard and controls.
- Using interface palette colors as a substitute for actual device RGB values.

## Review

Inspect rendered browser layouts at 1280×800, 1024 px wide, and a narrow mobile viewport. Check the keyboard's proportions and legends, navigation and feature hierarchy, dense controls, status distinctions, focus visibility, and that actual control messages and behavior still match the existing feature contract. A screenshot alone does not establish device-operation correctness.
