// SPDX-License-Identifier: GPL-3.0-or-later
import { bindingLabel, bindingChanges } from "./keymap_view.mjs";
import { node, button, field, choice, actions } from "./widgets.mjs";

export function createKeymapForm({ doc, runtime, redraw }) {
  let tool = "remap", preset = "Colemak";
  let shortcutKey = runtime.view.descriptor.shortcuts?.keys[0]?.usage ?? 4;
  const shortcutMods = new Set();

  function render(panel, view, { layer, key }, macroNames) {
    const editor = view.keymap, descriptor = view.descriptor;
    panel.append(node(doc, "h2", "Keys"));
    panel.append(actions(doc, editor, "keymap", input => runtime.intent(input)));
    const selected = descriptor.keys.find(item => item.id === key);
    const layerData = descriptor.layers.find(item => item.id === layer);
    const protectedKey = !selected?.writable || layerData?.read_only_keys.includes(key);
    const selectedCard = node(doc, "section", undefined, "key-target");
    selectedCard.append(node(doc, "h3", `${selected?.label ?? "Key"} · ${layerData?.label ?? "Layer"}`));
    const before = bindingLabel(descriptor, editor.baseline?.bindings?.[layer]?.[key], layer, key, macroNames).full;
    const after = bindingLabel(descriptor, editor.draft?.[layer]?.[key], layer, key, macroNames).full;
    selectedCard.append(node(doc, "p", protectedKey ? "Reserved onboard key" : before === after ? after : `${before} → ${after}`, "key-assignment"));
    panel.append(selectedCard);

    const tools = node(doc, "div", undefined, "key-tools");
    tools.setAttribute("role", "tablist");
    for (const [id, label] of [["remap", "Remap"], ["shortcut", "Shortcut"], ["presets", "Presets"]]) {
      const tab = button(doc, label, () => { tool = id; redraw(); }, false, tool === id ? "active" : "");
      tab.setAttribute("role", "tab"); tab.setAttribute("aria-selected", String(tool === id));
      tools.append(tab);
    }
    panel.append(tools);
    const form = node(doc, "div", undefined, "key-editor");
    if (tool === "remap") {
      const select = node(doc, "select");
      select.disabled = !editor.canEdit || protectedKey;
      const placeholder = node(doc, "option", "Choose action");
      placeholder.value = ""; select.append(placeholder);
      const groups = new Map();
      descriptor.actions.forEach((item, index) => {
        const category = item.category || "Other";
        let group = groups.get(category);
        if (!group) { group = node(doc, "optgroup"); group.label = category; groups.set(category, group); select.append(group); }
        const option = node(doc, "option", item.label); option.value = String(index); group.append(option);
      });
      select.value = "";
      select.addEventListener("change", () => {
        const action = descriptor.actions[Number(select.value)]?.action;
        if (action) void runtime.edit("keymap", { layer, key, action });
      });
      field(doc, form, "Action", select);
    } else if (tool === "shortcut") {
      const shortcuts = descriptor.shortcuts;
      if (!shortcuts) form.append(node(doc, "p", "Shortcuts unavailable", "muted"));
      else {
        const mods = node(doc, "div", undefined, "modifier-list");
        for (const modifier of shortcuts.modifiers) {
          const label = node(doc, "label", modifier.label);
          const input = node(doc, "input"); input.type = "checkbox"; input.checked = shortcutMods.has(modifier.usage);
          input.disabled = !editor.canEdit || protectedKey;
          input.addEventListener("change", () => { input.checked ? shortcutMods.add(modifier.usage) : shortcutMods.delete(modifier.usage); });
          label.prepend(input); mods.append(label);
        }
        form.append(mods);
        field(doc, form, "Key", choice(doc, shortcuts.keys.map(item => [item.usage, item.label]), shortcutKey,
          value => { shortcutKey = Number(value); }, !editor.canEdit || protectedKey));
        form.append(button(doc, "Stage shortcut", () => void runtime.edit("keymap", {
          layer, key, action: { Shortcut: { modifiers: [...shortcutMods], key: shortcutKey } },
        }), !editor.canEdit || protectedKey, "primary"));
      }
    } else {
      const scope = node(doc, "p", "Base layer letter keys", "muted"); form.append(scope);
      const presets = node(doc, "div", undefined, "preset-options");
      for (const name of ["Colemak", "Dvorak"]) {
        const option = button(doc, name, () => { preset = name; redraw(); }, false, preset === name ? "active" : "");
        option.setAttribute("aria-pressed", String(preset === name));
        presets.append(option);
      }
      form.append(presets);
      form.append(button(doc, `Stage ${preset}`, () => void runtime.intent({type:"stageTypingPreset", layer, preset}),
        !editor.canEdit || layer !== "base", "primary"));
      if (layer !== "base") form.append(node(doc, "p", "Choose Base to stage a preset.", "muted"));
    }
    if (protectedKey && tool !== "presets") form.append(node(doc, "p", "This key cannot be changed.", "muted"));
    panel.append(form);
    const changes = bindingChanges(descriptor, editor.baseline, editor.draft, macroNames);
    if (changes.length) {
      const review = node(doc, "div", undefined, "card keymap-changes");
      review.append(node(doc, "h3", "Pending changes"));
      for (const change of changes) review.append(node(doc, "p", `${change.layer} / ${change.key}: ${change.before} → ${change.after}`));
      panel.append(review);
    }
  }
  return { render };
}
