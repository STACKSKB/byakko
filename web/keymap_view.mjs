// SPDX-License-Identifier: GPL-3.0-or-later
// Presentation only: bindings and before-images remain owned by the core editor.
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const shortNames = {
  Calculator: "Calc", "Scroll Lock": "ScrLk", "Print Screen": "PrtSc",
  "Page Up": "PgUp", "Page Down": "PgDn", Backspace: "Bksp",
  Delete: "Del", Insert: "Ins", Escape: "Esc", Disabled: "Off",
  "Display Brightness Up": "Bright+", "Display Brightness Down": "Bright−",
  "Volume Up": "Vol+", "Volume Down": "Vol−", "Previous Track": "Prev",
  "Next Track": "Next", "Play/Pause": "Play",
};

export function bindingLabel(descriptor, action, layer, key, macroNames = {}) {
  if (action === undefined || action === null) return { full: "Unknown", compact: "Unknown" };
  const protectedKey = descriptor.layers.some(item => item.id === layer && item.read_only_keys.includes(key));
  const namedSlot = action.Macro && macroNames[`slot-${String(action.Macro.slot).padStart(2, "0")}`]?.trim();
  const catalog = descriptor.actions.find(item => same(item.action, action));
  let full = namedSlot || catalog?.label;
  if (!full) {
    if (action.Key !== undefined) full = descriptor.shortcuts?.keys.find(item => item.usage === action.Key)?.label ?? `Key ${action.Key}`;
    else if (action === "Disabled") full = "Disabled";
    else if (action.Macro) full = `Macro ${Number(action.Macro.slot) + 1}`;
    else if (action.Shortcut) {
      const caps = descriptor.shortcuts;
      const modifiers = action.Shortcut.modifiers.map(usage => caps?.modifiers.find(item => item.usage === usage)?.label);
      const target = caps?.keys.find(item => item.usage === action.Shortcut.key)?.label;
      full = target && modifiers.every(Boolean) ? [...modifiers, target].join("+") : "Shortcut";
    } else if (action.Named) full = action.Named.id;
    else if (action.Opaque) full = action.Opaque.label || "Unknown binding";
    else full = "Unknown";
  }
  const compact = action.Opaque ? (protectedKey ? "Onboard" : "Unknown") : shortNames[full] ?? full;
  return { full, compact };
}

export function bindingChanges(descriptor, baseline, draft, macroNames = {}) {
  if (!baseline || !draft) return [];
  const changes = [];
  for (const layer of descriptor.layers) {
    const before = baseline.bindings?.[layer.id] ?? {};
    const after = draft[layer.id] ?? {};
    const ids = new Set([...Object.keys(before), ...Object.keys(after)]);
    for (const key of descriptor.keys) {
      if (!ids.has(key.id) || same(before[key.id], after[key.id])) continue;
      changes.push({
        layer: layer.label, key: key.label,
        before: bindingLabel(descriptor, before[key.id], layer.id, key.id, macroNames).full,
        after: bindingLabel(descriptor, after[key.id], layer.id, key.id, macroNames).full,
      });
    }
  }
  return changes;
}
