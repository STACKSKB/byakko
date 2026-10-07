// SPDX-License-Identifier: GPL-3.0-or-later
import catalog from "./layouts.json" with { type: "json" };

export const layoutCatalog = catalog;
export const defaultLayout = { standard: "ansi", size: "full", legends: "qwerty" };

export function layoutFor({ standard, size, legends }) {
  const physical = catalog.standards[standard];
  const form = catalog.sizes[size];
  const legend = catalog.legends[legends];
  if (!physical || !form || !legend) throw new Error("Unknown keyboard layout choice");
  const labels = { ...catalog.labels, ...legend.keys, ...(standard === "iso" ? legend.iso : {}) };
  const keys = [];
  function key(code, x, y, width = 1, height = 1, shape = null) {
    keys.push({ id: code, code: code === "Fn" ? null : code,
      label: code === "Backspace" && width < 1.5 ? "Bksp" : labels[code] ?? code.replace(/^(Key|Digit|Numpad)/, ""),
      x, y, width, height, shape });
  }
  const alphaY = form.functionRow === "spaced" ? 1.5 : form.functionRow ? 1.25 : 0;
  physical.rows.forEach((row, index) => {
    let x = 0;
    for (const entry of row) {
      const [code, width = 1, height = 1, shape = null] = Array.isArray(entry) ? entry : [entry];
      key(code, x, alphaY + index, width, height, shape);
      x += width;
    }
  });
  if (form.functionRow) {
    key("Escape", 0, 0);
    for (let number = 1; number <= 12; number++) {
      const x = form.functionRow === "spaced" ? number + 1 + Math.floor((number - 1) / 4) * 0.5 : number + 0.25;
      key(`F${number}`, x, 0);
    }
  } else {
    // Escape replaces the grave key on these representative compact presets.
    const grave = keys.find(item => item.code === "Backquote");
    Object.assign(grave, { id: "Escape", code: "Escape", label: labels.Escape });
  }
  if (form.navigation === "tkl") {
    ["PrintScreen", "ScrollLock", "Pause"].forEach((code, i) => key(code, 15.5 + i, 0));
    ["Insert", "Home", "PageUp", "Delete", "End", "PageDown"].forEach((code, i) => key(code, 15.5 + i % 3, alphaY + Math.floor(i / 3)));
    key("ArrowUp", 16.5, alphaY + 3);
    ["ArrowLeft", "ArrowDown", "ArrowRight"].forEach((code, i) => key(code, 15.5 + i, alphaY + 4));
  }
  if (form.navigation === "compact") {
    ["Delete", "PageUp", "PageDown", "End"].forEach((code, i) => key(code, 15.25, alphaY + i));
    if (form.functionRow) key("Home", 14, 0);
    const shift = keys.find(item => item.code === "ShiftRight");
    shift.width -= 1;
    key("ArrowUp", 14, alphaY + 3);
    // Reuse the standard left modifiers and space; pack the right-hand controls.
    const compact = keys.filter(item => item.y !== alphaY + 4 || item.x < 10);
    keys.splice(0, keys.length, ...compact);
    key("AltRight", 10, alphaY + 4);
    key("Fn", 11, alphaY + 4);
    key("ControlRight", 12, alphaY + 4);
    ["ArrowLeft", "ArrowDown", "ArrowRight"].forEach((code, i) => key(code, 13.25 + i, alphaY + 4));
  }
  if (size === "60") {
    const rightMeta = keys.find(item => item.code === "MetaRight");
    Object.assign(rightMeta, { id: "Fn", code: null, label: "Fn" });
  }
  if (form.numpad) {
    const x = 19;
    ["NumLock", "NumpadDivide", "NumpadMultiply", "NumpadSubtract"].forEach((code, i) => key(code, x + i, alphaY));
    [[7, 8, 9], [4, 5, 6], [1, 2, 3]].forEach((row, i) => row.forEach((digit, j) => key(`Numpad${digit}`, x + j, alphaY + i + 1)));
    key("NumpadAdd", x + 3, alphaY + 1, 1, 2);
    key("NumpadEnter", x + 3, alphaY + 3, 1, 2);
    key("Numpad0", x, alphaY + 4, 2);
    key("NumpadDecimal", x + 2, alphaY + 4);
  }
  return keys;
}
