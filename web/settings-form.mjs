// SPDX-License-Identifier: GPL-3.0-or-later
import { pendingField, numberKind } from "./settings_view.mjs";
import { node, field, choice, actions } from "./widgets.mjs";

export function createSettingsForm({ doc, runtime }) {
  function render(panel, view) {
    panel.append(node(doc, "h2", "Settings"), node(doc, "p", "Onboard timing and behavior. Each setting saves independently after 500 ms.", "muted"));
    const state = view.settings;
    if (!state) return;
    const editor = state.editor;
    panel.append(actions(doc, editor, "settings", input => runtime.intent(input), { auto: true }));
    if (!editor.draft) {
      panel.append(node(doc, "p", editor.baseline?.content?.Opaque?.reason || "Read settings to edit them.", "muted"));
      return;
    }
    const pending = pendingField(editor);
    if (pending) panel.append(node(doc, "p", `Finish the staged ${state.capabilities.fields.find(item => item.id === pending)?.label ?? pending} change before editing another setting.`, "muted"));
    const list = node(doc, "div", undefined, "settings-list");
    for (const setting of state.capabilities.fields) {
      const value = editor.draft[setting.id];
      if (!value) continue;
      const card = node(doc, "div", undefined, "card setting-row");
      const title = node(doc, "div", undefined, "setting-title");
      title.append(node(doc, "h3", setting.label));
      if (pending === setting.id) title.append(node(doc, "span", "Pending", "setting-pending"));
      card.append(title);
      const locked = !editor.canEdit || Boolean(pending && pending !== setting.id);
      if (setting.kind === "Toggle") {
        const toggle = node(doc, "input");
        toggle.type = "checkbox"; toggle.checked = Boolean(value.Toggle); toggle.disabled = locked;
        toggle.addEventListener("change", () => void runtime.edit("settings", { id: setting.id, value: { Toggle: toggle.checked } }, true));
        const label = node(doc, "label", toggle.checked ? "Enabled" : "Disabled", "setting-toggle");
        toggle.setAttribute("aria-label", setting.label);
        label.prepend(toggle);
        card.append(label);
      } else {
        const limits = numberKind(setting);
        if (!limits) continue;
        const amount = Number(value.Number);
        if (limits.disabled_zero) {
          const stateRow = node(doc, "div", undefined, "setting-controls");
          const enabled = choice(doc,
            [["enabled", "Enabled"], ["disabled", "Disabled"]], amount === 0 ? "disabled" : "enabled",
            choiceValue => void runtime.edit("settings", { id: setting.id, value: { Number: choiceValue === "disabled" ? 0 : limits.min } }, true), locked);
          enabled.setAttribute("aria-label", `${setting.label} state`);
          field(doc, stateRow, "State", enabled);
          card.append(stateRow);
        }
        const controls = node(doc, "div", undefined, "setting-controls");
        const slider = node(doc, "input");
        slider.type = "range"; slider.min = String(limits.min); slider.max = String(limits.max);
        slider.step = String(limits.step); slider.value = String(amount || limits.min);
        slider.disabled = locked || amount === 0;
        slider.setAttribute("aria-label", `${setting.label} slider`);
        slider.addEventListener("change", () => void runtime.edit("settings", { id: setting.id, value: { Number: Number(slider.value) } }, true));
        const numberInput = node(doc, "input");
        numberInput.type = "number"; numberInput.value = String(amount || limits.min);
        numberInput.min = String(limits.min); numberInput.max = String(limits.max);
        numberInput.disabled = locked || amount === 0;
        numberInput.step = String(limits.step);
        numberInput.setAttribute("aria-label", `${setting.label} value`);
        numberInput.addEventListener("change", () => {
          const next = Number(numberInput.value);
          if (!Number.isInteger(next) || next < limits.min || next > limits.max ||
              (next - limits.min) % limits.step !== 0) {
            numberInput.value = String(amount || limits.min);
            runtime.announce(`Choose ${setting.label} from ${limits.min} to ${limits.max} ${limits.unit}.`);
            return;
          }
          void runtime.edit("settings", { id: setting.id, value: { Number: next } }, true);
        });
        controls.append(slider, numberInput, node(doc, "span", limits.unit, "muted"));
        card.append(controls);
      }
      list.append(card);
    }
    panel.append(list);
  }

  return { render };
}
