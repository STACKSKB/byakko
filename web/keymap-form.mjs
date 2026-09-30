// SPDX-License-Identifier: GPL-3.0-or-later
import { bindingLabel, bindingChanges } from "./keymap_view.mjs";
import { node, button, field, choice, actions } from "./widgets.mjs";

export function createKeymapForm({ doc, runtime }) {
  let shortcutKey = runtime.view.descriptor.shortcuts?.keys[0]?.usage ?? 4;
  const shortcutMods = new Set();

  function render(panel, view, { layer, key }, macroNames) {
    const editor = view.keymap;
    const descriptor = view.descriptor;
    panel.append(node(doc, "h2", "Keymap"), node(doc, "p", "Select a key, choose an action, then save.", "muted"));
    panel.append(actions(doc, editor, "keymap", input => runtime.intent(input)));
    const selected = descriptor.keys.find(item => item.id === key);
    const layerData = descriptor.layers.find(item => item.id === layer);
    const protectedKey = !selected?.writable || layerData?.read_only_keys.includes(key);
    const card = node(doc, "div", undefined, "card");
    card.append(node(doc, "h3", `${selected?.label ?? "Key"} · ${layerData?.label ?? "Layer"}`));
    card.append(node(doc, "p", protectedKey ? "Reserved for an onboard command" : `Current: ${bindingLabel(descriptor, editor.draft?.[layer]?.[key], layer, key, macroNames).full}`, "muted"));
    const select = node(doc, "select");
    select.disabled = !editor.canEdit || protectedKey;
    const placeholder = node(doc, "option", "Choose action…");
    placeholder.value = "";
    select.append(placeholder);
    const groups = new Map();
    descriptor.actions.forEach((item, index) => {
      const category = item.category || "Other";
      let container = groups.get(category);
      if (!container) { container = node(doc, "optgroup"); container.label = category; groups.set(category, container); select.append(container); }
      const option = node(doc, "option", item.label);
      option.value = String(index);
      container.append(option);
    });
    select.value = "";
    select.addEventListener("change", () => {
      const action = descriptor.actions[Number(select.value)]?.action;
      if (action) void runtime.edit("keymap", { layer, key, action });
    });
    field(doc, card, "Action", select);
    const shortcuts = descriptor.shortcuts;
    if (shortcuts && !protectedKey) {
      const box = node(doc, "details", undefined, "shortcut-box");
      box.append(node(doc, "summary", "Custom shortcut"));
      const mods = node(doc, "div", undefined, "modifier-list");
      for (const modifier of shortcuts.modifiers) {
        const label = node(doc, "label", modifier.label);
        const input = node(doc, "input"); input.type = "checkbox"; input.checked = shortcutMods.has(modifier.usage);
        input.disabled = !editor.canEdit;
        input.addEventListener("change", () => { input.checked ? shortcutMods.add(modifier.usage) : shortcutMods.delete(modifier.usage); });
        label.prepend(input); mods.append(label);
      }
      box.append(mods);
      field(doc, box, "Target key", choice(doc, shortcuts.keys.map(item => [item.usage, item.label]), shortcutKey, value => { shortcutKey = Number(value); }, !editor.canEdit));
      box.append(button(doc, "Assign shortcut", () => void runtime.edit("keymap", { layer, key, action: { Shortcut: { modifiers: [...shortcutMods], key: shortcutKey } } }), !editor.canEdit));
      card.append(box);
    }
    panel.append(card);
    const changes = bindingChanges(descriptor, editor.baseline, editor.draft, macroNames);
    if (changes.length) {
      const review = node(doc, "div", undefined, "card keymap-changes");
      review.append(node(doc, "h3", "Changes to save"));
      for (const change of changes) review.append(node(doc, "p", `${change.layer} / ${change.key}: ${change.before} → ${change.after}`));
      panel.append(review);
    }
  }

  return { render };
}
