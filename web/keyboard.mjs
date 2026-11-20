// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, same, hex } from "./widgets.mjs";
import { bindingLabel } from "./keymap_view.mjs";
import { boardBounds, keyRectangle } from "./key-geometry.mjs";

// Photographed MDA Lavender grouping. Physical colours never enter device values.
const lavenderKeys = new Set(["Esc", "F5", "F6", "F7", "F8", "PrtSc", "ScrLk", "Pause",
  "Tab", "Caps", "LShift", "RShift", "LCtrl", "RCtrl", "LWin", "RWin", "LAlt", "RAlt",
  "Fn", "Menu", "Backspace", "Enter", "Insert", "Home", "PgUp", "Delete", "End", "PgDn",
  "Left", "Down", "Up", "Right"]);

function readableOn(color) {
  const channels = [1, 3, 5].map(at => parseInt(color.slice(at, at + 2), 16) / 255);
  const light = channels.map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4)
    .reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
  return light > .18 ? "#000000" : "#ffffff";
}

export function createKeyboard({ doc, selectLayer, selectKey, redraw }) {
  function render(keyboard, view, {tab, layer, key}, macroNames, lighting) {
    keyboard.replaceChildren();
    const keys = view.descriptor.keys.filter(item => item.visible);
    const { width, height } = boardBounds(keys);
    const pixelWidth = width * 43 + 14, pixelHeight = height * 43 + 14;
    const heading = node(doc, "div", undefined, "keyboard-heading");
    const label = node(doc, "div", undefined, "board-name");
    label.append(node(doc, "strong", view.descriptor.device_name), node(doc, "small", "Physical keys"));
    const controls = node(doc, "div", undefined, "keyboard-controls");
    const layers = node(doc, "div", undefined, "layer-tabs");
    layers.setAttribute("aria-label", "Keyboard layer");
    for (const entry of view.descriptor.layers) {
      const layerButton = button(doc, entry.label, () => selectLayer(entry.id), false, entry.id === layer ? "active" : "");
      layerButton.setAttribute("aria-pressed", String(entry.id === layer));
      layers.append(layerButton);
    }
    controls.append(layers);
    if (tab === "lighting") lighting.renderMode(controls, view);
    heading.append(label, controls);

    const scroll = node(doc, "div", undefined, "keyboard-scroll");
    scroll.setAttribute("aria-label", "Physical keyboard");
    const board = node(doc, "div", undefined, "keyboard");
    board.style.aspectRatio = `${pixelWidth} / ${pixelHeight}`;
    const { pictureMode, previewColors } = lighting.keyboardState(view);
    const colors = view.picture?.editor?.draft ?? {};
    for (const item of keys) {
      const mapped = (tab === "keymap" || tab === "macros") && view.keymap.draft?.[layer]?.[item.id]
        ? bindingLabel(view.descriptor, view.keymap.draft[layer][item.id], layer, item.id, macroNames) : null;
      const changed = mapped && !same(view.keymap.baseline?.bindings?.[layer]?.[item.id], view.keymap.draft[layer][item.id]);
      const stock = lavenderKeys.has(item.label.replaceAll(" ", "")) ? "lavender" : "ivory";
      const element = button(doc, undefined, () => {
        selectKey(item.id);
        if (tab !== "lighting" || !lighting.paintKey(item.id)) redraw();
      }, false, `key ${stock} ${key === item.id ? "selected" : ""} ${changed ? "changed" : ""} ${view.descriptor.layers.find(entry => entry.id === layer)?.read_only_keys.includes(item.id) || !item.writable ? "protected" : ""}`);
      element.dataset.key = item.id;
      element.append(node(doc, "span", item.label, "key-legend"));
      if (mapped && mapped.compact !== item.label) element.append(node(doc, "span", mapped.compact, "key-binding"));
      element.title = mapped ? `${item.label} · ${mapped.full}` : item.label;
      element.setAttribute("aria-label", mapped && mapped.compact !== item.label ? `${item.label} · ${mapped.full}` : item.label);
      element.setAttribute("aria-pressed", String(key === item.id));
      for (const [property, value] of Object.entries(keyRectangle(item))) {
        element.style[property] = `${value / (property === "left" || property === "width" ? pixelWidth : pixelHeight) * 100}%`;
      }
      const color = previewColors?.[item.id] ?? (tab === "lighting" && pictureMode && colors[item.id] ? hex(colors[item.id]) : null);
      if (color && /^#[0-9a-f]{6}$/i.test(color)) {
        element.classList.add("has-color");
        element.style.setProperty("--key-color", color);
        element.style.setProperty("--key-color-ink", readableOn(color));
      }
      board.append(element);
    }
    scroll.append(board);
    const footer = node(doc, "div", undefined, "keyboard-footer");
    const legend = node(doc, "div", undefined, "keyboard-legend");
    legend.append(node(doc, "span", "▣ Selected"), node(doc, "span", "• Pending"));
    const chooser = node(doc, "label", "Key", "key-chooser");
    const select = node(doc, "select");
    select.setAttribute("aria-label", "Choose physical key");
    for (const item of keys) {
      const option = node(doc, "option", item.label);
      option.value = item.id;
      option.selected = item.id === key;
      select.append(option);
    }
    select.addEventListener("change", () => { selectKey(select.value); redraw(); });
    chooser.append(select);
    footer.append(legend, chooser);
    keyboard.append(heading, scroll, footer);
  }
  return { render };
}
