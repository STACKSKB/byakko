---
name: byakko-ui
description: Apply Byakko's browser visual direction when creating or refining its interface, while preserving its established feature layout and interaction contract.
---

# Byakko UI

Use this skill for visual design or implementation work on Byakko's optional browser client. Read the repository's [UX reference](../../docs/ux.md) for the color direction, physical keyboard cues, source observations, and review sizes.

Keep existing navigation, feature placement, keyboard workspace, status meanings, and device-operation behavior intact. This skill guides presentation; it does not authorize protocol changes or device writes. Treat reference sites as inspiration for general hierarchy only. Do not reuse their branding or assets, and do not claim an unaudited reference was inspected.

When implementing, keep the real keyboard geometry and physical legends legible, group controls with whitespace and light rules, and retain all supported controls including every lighting mode. Review rendered layouts at the sizes in the UX reference and verify that visible controls still emit the intended existing messages.

Use the semantic tokens in `web/palette.css` for interface colours. Check both
light and dark themes; keep physical keycap colours and device RGB separate
from appearance preferences. Add palettes through token values, not parallel
component styles or device settings.

For a WordPress embed, follow the WordPress section of the UX reference: parent
page typography, white surfaces and a neutral indigo accent, with page-owned
scrolling and visible dialogs. Keep the standalone visual direction intact.
