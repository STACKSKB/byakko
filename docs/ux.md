# Browser visual direction

This reference guides the optional browser client's visual styling. It does not replace the approved screenshots and interaction flows or authorize changes to feature placement, device behavior, or transport policy. Keep the persistent keyboard workspace and existing feature navigation recognizable while refining visual hierarchy and surface treatment.

## Direction

Use a warm, restrained workshop feel: an off-white application canvas, white work surfaces, clear ink, fine separators, and a muted indigo accent. The keyboard remains the visual anchor and should feel like a real tool being configured. Prefer compact system typography, direct labels, and a few well-spaced groups over decorative chrome.

Suggested tokens:

| Role | Color |
| --- | --- |
| Canvas | `#f4f3ef` |
| Work surface | `#ffffff` |
| Main text | `#272823` |
| Secondary text | `#686962` |
| Rule | `#d8d8d0` |
| Keycap indigo | `#626991` |
| Focus accent | `#565b8f` |

Treat these as starting values, not a mandate to make every component monochrome. Keep accessible contrast and meaningful focus visible. Use compact controls around 32 px high on desktop and 40 px on touch layouts. Use whitespace and hairline rules to group related content. Reserve stronger fills for selected or primary actions.

## Appearance preferences

The browser's Appearance dialog offers System, Light and Dark themes, plus
Indigo, Slate and Forest palettes. System follows the operating system's colour
preference, including changes during a visit. The choice is stored locally in
the browser, separately from keyboard settings and backups. If storage is
unavailable, the choice still applies to the current visit.

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
visible outer-page region. Retain the deliberate horizontal keyboard scroll
on narrow screens and bounded macro lists. Place the standalone link below the
embed. WordPress appearance defaults to Light/Neutral and uses a separate
storage key; explicit light/dark and alternate palette choices remain available.
The Neutral palette option is shown only in WordPress.

## Keyboard illustration

Use warm light keycaps, indigo functional groups, quiet outlines, and only mild
depth in the keyboard illustration. Keep legends readable and show mapped key
labels without hiding the physical legends.

Preserve the full Nia87 layout and make its available actions discoverable. Lighting mode selection must continue to expose all 23 modes. Keep connection, loading, dirty, conflict, and operation status easy to distinguish; do not make color the sole status signal.

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
