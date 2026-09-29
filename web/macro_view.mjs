// SPDX-License-Identifier: GPL-3.0-or-later
// Local presentation only. Core owns occupancy and candidate selection.
export function librarySlots(capabilities, occupancy, keymap, selected) {
  const actions = Object.values(keymap ?? {}).flatMap(bindings => Object.values(bindings));
  const bound = new Set(capabilities.bindings
    .filter(binding => actions.some(action => JSON.stringify(action) === JSON.stringify(binding.action)))
    .map(binding => binding.slot));
  const visible = capabilities.slots.filter(slot =>
    occupancy[slot.id] === "Configured" || occupancy[slot.id] === "Opaque"
    || bound.has(slot.id) || slot.id === selected);
  return { visible, bound };
}
