// SPDX-License-Identifier: GPL-3.0-or-later
export const text = error => String(error?.message ?? error);
export const hex = rgb => `#${rgb.map(byte => byte.toString(16).padStart(2, "0")).join("")}`;
export const rgb = color => [1, 3, 5].map(at => Number.parseInt(color.slice(at, at + 2), 16));
export const range = value => value ? [value.start, value.end] : [0, 0];
export const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
export const editable = entry => entry?.editor ?? entry;

export function node(doc, tag, label, className) {
  const element = doc.createElement(tag);
  if (label !== undefined) element.textContent = label;
  if (className) element.className = className;
  return element;
}

export function button(doc, label, action, disabled = false, className = "") {
  const element = node(doc, "button", label, className);
  element.type = "button";
  element.disabled = disabled;
  element.addEventListener("click", action);
  return element;
}

export function field(doc, parent, label, control) {
  const wrapper = node(doc, "label", undefined, "field");
  wrapper.append(node(doc, "span", label), control);
  parent.append(wrapper);
  return control;
}

export function choice(doc, options, current, changed, disabled = false) {
  const select = node(doc, "select");
  select.disabled = disabled;
  for (const [value, label] of options) {
    const option = node(doc, "option", label);
    option.value = String(value);
    select.append(option);
  }
  select.value = String(current ?? "");
  select.addEventListener("change", () => changed(select.value));
  return select;
}

export function number(doc, value, min, max, changed, disabled = false) {
  const input = node(doc, "input");
  input.type = "number";
  input.value = String(value ?? min);
  input.min = String(min);
  input.max = String(max);
  input.disabled = disabled;
  input.addEventListener("change", () => changed(Number(input.value)));
  return input;
}

export function actions(doc, editor, feature, dispatch, { auto = false, revert } = {}) {
  const row = node(doc, "div", undefined, "actions");
  row.append(button(doc, "Read", () => void dispatch({ type: "read", feature }), !editor?.canRead));
  row.append(button(doc, "Revert", () => void (revert ? revert() : dispatch({ type: "revert", feature })), !editor?.canRevert));
  row.append(button(doc, "Save", () => void dispatch({ type: "apply", feature }), !editor?.canApply, "primary"));
  if (auto) row.append(node(doc, "span", "Changes save after 500 ms", "muted"));
  return row;
}

export function download(doc, win, filename, content) {
  const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
  const anchor = node(doc, "a");
  anchor.href = url; anchor.download = filename; anchor.click();
  win.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
