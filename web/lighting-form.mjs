// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, field, choice, number, actions, range, hex, rgb } from "./widgets.mjs";
import { createHostForm } from "./host-form.mjs";

export function createLightingForm({ doc, win, runtime, redraw, navigate }) {
  const host = createHostForm({ doc, win, runtime, redraw, navigate });
  let showHostLighting = false, paintColor = "#ffffff", paintReady = false;

  function keyboardState(view) {
    const lighting = view.lighting;
    return {
      pictureMode: !showHostLighting && lighting?.editor.draft?.effect === view.picture?.capabilities?.lighting_effect,
      modeLabel: showHostLighting ? host.selectedMode(view)?.label :
        lighting?.capabilities.effects.find(effect => effect.id === lighting.editor.draft?.effect)?.label,
    };
  }

  function renderMode(heading, view) {
    const lighting = view.lighting;
    if (!lighting) return;
    const caps = lighting.capabilities;
    const choices = [
      ...caps.effects.map(effect => [`onboard:${effect.id}`, effect.label]),
      ...caps.host_modes.map(mode => [`host:${mode.id}`, mode.label]),
    ];
    const selected = showHostLighting ? `host:${host.selectedMode(view)?.id}` :
      lighting.editor.draft ? `onboard:${lighting.editor.draft.effect}` : "";
    field(doc, heading, "Lighting mode", choice(doc, choices, selected, async value => {
      if (value.startsWith("host:")) {
        showHostLighting = true; host.select(value.slice(5)); paintReady = false;
        redraw();
      } else {
        const result = await runtime.edit("lighting", { Effect: value.slice(8) }, true);
        if (result?.ok) { showHostLighting = false; paintReady = false; redraw(); }
      }
    }, !lighting.editor.canSelectEffect || runtime.hostBusy));
  }

  function paintKey(key) {
    const view = runtime.view;
    if (!keyboardState(view).pictureMode || !paintReady || !view.picture?.editor?.canEdit ||
        !view.picture.capabilities.keys.includes(key)) return false;
    void runtime.edit("picture", { Color: { key, color: rgb(paintColor) } }, true);
    return true;
  }

  function render(panel, view, { key }) {
    const state = view.lighting;
    panel.append(node(doc, "h2", "Lighting"), node(doc, "p", "Onboard effects, brightness and per-key RGB.", "muted"));
    if (!state) return;
    const editor = state.editor, draft = editor.draft, caps = state.capabilities;
    panel.append(actions(doc, editor, "lighting", input => runtime.intent(input), { auto: true }));
    if (showHostLighting) { host.render(panel, view); return; }
    if (!draft) {
      const content = editor.baseline?.content;
      panel.append(node(doc, "p", content?.HostActive
        ? "A host lighting mode is stored on the keyboard. Select an onboard lighting mode above to replace it."
        : content?.Opaque?.reason ?? "Read lighting to edit it.", "muted"));
      return;
    }
    const effect = caps.effects.find(item => item.id === draft.effect);
    const card = node(doc, "div", undefined, "card form-grid");
    card.append(node(doc, "h3", effect?.label ?? "Onboard lighting"));
    if (effect?.options.length) field(doc, card, draft.effect === view.picture?.capabilities?.lighting_effect ? "Layer" : "Style / direction", choice(doc, effect.options.map(item => [item.id, item.label]), draft.option, value => void runtime.edit("lighting", { Option: value }, true), !editor.canEdit));
    for (const [name, property] of [["Brightness", "brightness"], ["Speed", "speed"]]) {
      if (!effect?.[property]) continue;
      const [min, max] = range(effect[property]);
      field(doc, card, name, number(doc, draft[property], min, max, value => void runtime.edit("lighting", { [name]: value }, true), !editor.canEdit));
    }
    if (effect?.color) {
      const fixed = draft.color?.Rgb;
      const color = node(doc, "input"); color.type = "color"; color.value = fixed ? hex(fixed) : "#ffffff";
      color.disabled = !editor.canEdit || draft.color === "Rainbow";
      color.addEventListener("input", () => { paintColor = color.value; });
      color.addEventListener("change", () => void runtime.edit("lighting", { Color: { Rgb: rgb(color.value) } }, true));
      field(doc, card, "Color", color);
      if (effect.color === "FixedOrRainbow" || effect.color === "Rainbow") {
        const label = node(doc, "label", "Rainbow");
        const checkbox = node(doc, "input"); checkbox.type = "checkbox"; checkbox.checked = draft.color === "Rainbow";
        checkbox.disabled = !editor.canEdit;
        checkbox.addEventListener("change", () => void runtime.edit("lighting", { Color: checkbox.checked ? "Rainbow" : { Rgb: rgb(color.value) } }, true));
        label.prepend(checkbox); card.append(label);
      }
    }
    panel.append(card);
    const pictureEffect = view.picture?.capabilities?.lighting_effect;
    if (pictureEffect && draft.effect === pictureEffect) {
      const pictureEditor = view.picture.editor;
      const colors = node(doc, "div", undefined, "card");
      colors.append(node(doc, "h3", "Per-key colors"));
      colors.append(node(doc, "p", "Load this layer, select a key on the keyboard, then choose its color.", "muted"));
      colors.append(button(doc, "Load selected layer", () => void runtime.intent({ type: "preparePicture" }), view.connection.kind !== "connected" || view.busy || editor.dirty));
      const picked = pictureEditor.draft?.[key];
      const picker = node(doc, "input"); picker.type = "color"; picker.value = paintReady ? paintColor : picked ? hex(picked) : paintColor;
      picker.disabled = !pictureEditor.canEdit || !view.picture.capabilities.keys.includes(key);
      picker.addEventListener("input", () => { paintColor = picker.value; });
      picker.addEventListener("change", () => { paintReady = true; void runtime.edit("picture", { Color: { key, color: rgb(picker.value) } }, true); });
      field(doc, colors, `Paint color · ${view.descriptor.keys.find(item => item.id === key)?.label ?? "key"}`, picker);
      if (paintReady) colors.append(button(doc, "Stop painting", () => { paintReady = false; redraw(); }));
      colors.append(actions(doc, pictureEditor, "picture", input => runtime.intent(input), { auto: true }));
      panel.append(colors);
    }
  }

  return { render, renderMode, keyboardState, paintKey, destroy: () => host.destroy(), stopHost: () => host.stop() };
}
