// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, same, hex } from "./widgets.mjs";
import { bindingLabel } from "./keymap_view.mjs";
// The Nia87's stock ivory/indigo keycap grouping, independent of RGB values.
const indigoKeys = new Set(["Esc", "F5", "F6", "F7", "F8", "PrtSc", "ScrLk", "Pause",
  "Tab", "Caps", "LShift", "RShift", "LCtrl", "RCtrl", "LWin", "RWin", "LAlt", "RAlt",
  "Fn", "Menu", "Backspace", "Enter", "Insert", "Home", "PgUp", "Delete", "End", "PgDn",
  "Left", "Down", "Up", "Right"]);

export function createKeyboard({ doc, selectLayer, selectKey, redraw }) {
  function render(keyboard, view, {tab,layer,key}, macroNames, lighting) {
    keyboard.replaceChildren();
    const heading = node(doc, "div", undefined, "keyboard-heading");
    const { pictureMode, modeLabel } = lighting.keyboardState(view);
    heading.append(node(doc, "div", `${view.descriptor.device_name} · ${tab === "lighting" ? modeLabel ?? "Lighting" : tab === "macros" ? "Assignment target" : tab === "keymap" ? "Key bindings" : "Keyboard overview"}`));
    if (tab === "lighting") lighting.renderMode(heading, view);
    if (tab === "keymap" || tab === "macros") {
      const layers = node(doc, "div", undefined, "layer-tabs");
      for (const entry of view.descriptor.layers) layers.append(button(doc, entry.label, () => selectLayer(entry.id), false, entry.id === layer ? "active" : ""));
      heading.append(layers);
    }
    const scroll = node(doc, "div", undefined, "keyboard-scroll");
    const board = node(doc, "div", undefined, "keyboard");
    const keys = view.descriptor.keys.filter(item => item.visible);
    const width = Math.max(...keys.map(item => item.x + item.width), 1);
    const height = Math.max(...keys.map(item => item.y + item.height), 1);
    board.style.width = `${width * 43 + 14}px`;
    board.style.height = `${height * 43 + 14}px`;
    const colors = view.picture?.editor?.draft ?? {};
    for (const item of keys) {
      const mapped = (tab === "keymap" || tab === "macros") && view.keymap.draft?.[layer]?.[item.id]
        ? bindingLabel(view.descriptor, view.keymap.draft[layer][item.id], layer, item.id, macroNames) : null;
      const changed = mapped && !same(view.keymap.baseline?.bindings?.[layer]?.[item.id], view.keymap.draft[layer][item.id]);
      const element = button(doc, undefined, () => {
        selectKey(item.id);
        if (tab !== "lighting" || !lighting.paintKey(item.id)) redraw();
      }, false, `key ${indigoKeys.has(item.label) ? "indigo" : "ivory"} ${key === item.id ? "selected" : ""} ${changed ? "changed" : ""} ${view.descriptor.layers.find(entry => entry.id === layer)?.read_only_keys.includes(item.id) || !item.writable ? "protected" : ""}`);
      element.append(node(doc, "span", item.label, "key-legend"));
      if (mapped && mapped.compact !== item.label) element.append(node(doc, "span", mapped.compact, "key-binding"));
      element.title = mapped ? `${item.label} · ${mapped.full}` : item.label;
      element.setAttribute("aria-label", mapped && mapped.compact !== item.label ? `${item.label} · ${mapped.full}` : item.label);
      element.setAttribute("aria-pressed", String(key === item.id));
      element.style.left = `${item.x * 43 + 7}px`;
      element.style.top = `${item.y * 43 + 7}px`;
      element.style.width = `${item.width * 43 - 4}px`;
      element.style.height = `${item.height * 43 - 4}px`;
      if (tab === "lighting" && pictureMode && colors[item.id]) {
        element.classList.add("has-color");
        element.style.setProperty("--key-color", hex(colors[item.id]));
      }
      board.append(element);
    }
    scroll.append(board);
    keyboard.append(heading, scroll);
  }

  return { render };
}
