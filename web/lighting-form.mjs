// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, field, choice, number, actions, range, hex, rgb } from "./widgets.mjs";
import { createHostForm } from "./host-form.mjs";

export function createLightingForm({ doc, win, runtime, codec, redraw, navigate, confirmDiscard }) {
  const host = createHostForm({ doc, win, runtime, redraw, navigate });
  let showHostLighting = false, paintColor = "#ffffff", paintMode = "Inspect";
  const looks = { id: null, corrections: {}, replace: false, target: null, connection: null, notice: "", applying: false };
  const paintedBySelector = new Map();
  let paintedConnection = null, paintBlocked = false;
  let cachedBrush = null, cachedCatalog = [];
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const picture = view => view.picture?.editor;
  const pictureKeys = view => view.picture?.capabilities?.keys ?? [];
  const pictureValues = view => picture(view)?.draft ?? null;
  const baseValues = view => picture(view)?.baseline?.content?.Editable ?? null;
  const selectorContext = view => JSON.stringify([view.connection.generation, picture(view)?.baseline?.backend_id,
    picture(view)?.baseline?.context_revision, view.lighting?.editor?.draft?.option]);
  function paintedKeys(view) {
    const identity = runtime.identity();
    if (!paintedConnection || paintedConnection.device !== identity.device || paintedConnection.generation !== identity.generation) {
      paintedBySelector.clear(); paintedConnection = identity; paintBlocked = false;
    }
    const selector = selectorContext(view);
    if (!paintedBySelector.has(selector)) paintedBySelector.set(selector, new Set());
    return paintedBySelector.get(selector);
  }
  function catalog() {
    if (cachedBrush === paintColor) return cachedCatalog;
    try {
      const entries = JSON.parse(codec?.picture_looks?.(...rgb(paintColor)) ?? "[]");
      cachedCatalog = Array.isArray(entries) ? entries : [];
    } catch { cachedCatalog = []; }
    cachedBrush = paintColor;
    return cachedCatalog;
  }
  function completeMap(entry, view) {
    const keys = pictureKeys(view);
    const colors = entry?.colors;
    if (!colors || !keys.length || Object.keys(colors).length !== keys.length ||
        keys.some(key => !Array.isArray(colors[key]) || colors[key].length !== 3 ||
          colors[key].some(byte => !Number.isInteger(byte) || byte < 0 || byte > 255))) return null;
    return colors;
  }
  function armed(view) {
    const lighting = view.lighting?.editor?.draft;
    const setting = view.lighting?.editor?.baseline?.content?.Editable;
    return Boolean(view.connection.kind === "connected" && picture(view)?.canEdit && pictureValues(view) &&
      baseValues(view) && lighting?.effect === view.picture?.capabilities?.lighting_effect &&
      setting?.effect === lighting?.effect && setting?.option === lighting?.option &&
      same(view.picture?.editor?.baseline?.context_revision, view.lighting?.editor?.baseline?.picture_context) &&
      pictureKeys(view).every(key => Object.hasOwn(pictureValues(view), key)));
  }
  function stale(view) { return looks.target !== null && (looks.target !== selectorContext(view) || !runtime.isCurrent(looks.connection)); }
  function pendingKeys(view) {
    const base = baseValues(view), draft = pictureValues(view);
    return new Set(pictureKeys(view).filter(key => base && draft && !same(base[key], draft[key])));
  }
  function previewMap(view) {
    if (!looks.id || stale(view) || !armed(view)) return null;
    const map = completeMap(catalog().find(entry => entry.id === looks.id), view);
    if (!map) return null;
    const result = Object.fromEntries(pictureKeys(view).map(key => [key, [...map[key]]]));
    if (!looks.replace) for (const key of new Set([...paintedKeys(view), ...pendingKeys(view)])) {
      if (pictureValues(view)?.[key]) result[key] = [...pictureValues(view)[key]];
    }
    for (const [key, color] of Object.entries(looks.corrections)) if (Object.hasOwn(result, key)) result[key] = [...color];
    return result;
  }
  function previewColors(view) {
    const map = previewMap(view);
    return map ? Object.fromEntries(Object.entries(map).map(([key, value]) => [key, hex(value)])) : null;
  }

  function keyboardState(view) {
    const lighting = view.lighting;
    const showingHost = showHostLighting && runtime.hostModesAvailable;
    return {
      pictureMode: !showingHost && lighting?.editor.draft?.effect === view.picture?.capabilities?.lighting_effect,
      modeLabel: showingHost ? host.selectedMode(view)?.label :
        lighting?.capabilities.effects.find(effect => effect.id === lighting.editor.draft?.effect)?.label,
      previewColors: !showingHost && !stale(view) ? previewColors(view) : null,
    };
  }

  function renderMode(heading, view) {
    const lighting = view.lighting;
    if (!lighting) return;
    const caps = lighting.capabilities;
    const choices = [
      ...caps.effects.map(effect => [`onboard:${effect.id}`, effect.label]),
      ...(runtime.hostModesAvailable ? caps.host_modes.map(mode => [`host:${mode.id}`, mode.label]) : []),
    ];
    const selected = showHostLighting && runtime.hostModesAvailable ? `host:${host.selectedMode(view)?.id}` :
      lighting.editor.draft ? `onboard:${lighting.editor.draft.effect}` : "";
    field(doc, heading, "Lighting mode", choice(doc, choices, selected, async value => {
      if (value.startsWith("host:")) {
        showHostLighting = true; host.select(value.slice(5)); paintMode = "Inspect";
        redraw();
      } else {
        const result = await runtime.edit("lighting", { Effect: value.slice(8) }, true);
        if (result?.ok) { showHostLighting = false; paintMode = "Inspect"; redraw(); }
      }
    }, !lighting.editor.canSelectEffect || runtime.hostBusy));
  }

  function selectLook(id) {
    const view = runtime.view;
    if (!armed(view)) { looks.notice = "Load per-key colors first."; redraw(); return; }
    const entry = catalog().find(item => item.id === id);
    if (!completeMap(entry, view)) { looks.notice = "Look unavailable for this keyboard."; redraw(); return; }
    looks.id = id;
    looks.target = selectorContext(view);
    looks.connection = runtime.identity();
    looks.corrections = {};
    looks.notice = `${entry.name} preview. Keyboard unchanged.`;
    redraw();
  }

  function discardLook() {
    looks.id = null;
    looks.corrections = {};
    looks.replace = false;
    looks.target = null;
    looks.connection = null;
    looks.notice = "Look preview discarded. Painted keys kept.";
    redraw();
  }

  async function confirmDiscardLook() {
    if (!looks.id) return;
    const accepted = await (confirmDiscard?.({ title: "Discard look preview?", description: "Painted keys stay as they are." }) ?? Promise.resolve(true));
    if (accepted) discardLook();
  }

  async function applyLook() {
    const view = runtime.view;
    if (!looks.id || looks.applying || stale(view) || !armed(view) || view.busy) return;
    const colors = previewMap(view);
    const baseline = picture(view)?.baseline;
    if (!colors || !baseline) { looks.notice = "Look unavailable for this keyboard."; redraw(); return; }
    looks.applying = true;
    const before = pictureValues(view) ? structuredClone(pictureValues(view)) : null;
    const target = runtime.identity();
    looks.notice = "Applying look…";
    redraw();
    try {
      const staged = await runtime.intent({ type: "stagePicture", snapshot: { ...baseline, content: { Editable: colors } } });
      if (!staged?.ok) { looks.notice = "Look kept. Check per-key colors before retrying."; return; }
      const saved = await runtime.intent({ type: "apply", feature: "picture" });
      if (saved?.ok && saved.outcome?.kind === "pictureSaved") {
        if (runtime.isCurrent(target) && selectorContext(runtime.view) === looks.target) {
          const painted = paintedKeys(runtime.view);
          if (looks.replace) painted.clear();
          for (const key of Object.keys(looks.corrections)) painted.add(key);
        }
        looks.id = null; looks.corrections = {}; looks.replace = false; looks.target = null; looks.connection = null;
        looks.notice = "Look sent. No readback.";
      } else {
        if (runtime.isCurrent(target) && before) {
          const restored = await runtime.intent({ type: "stagePicture", snapshot: { ...baseline, content: { Editable: before } } });
          paintBlocked = !restored?.ok;
        }
        looks.notice = "Look kept. Check per-key colors before retrying.";
      }
    } finally { looks.applying = false; redraw(); }
  }

  function thumbnail(entry, view) {
    const keys = view.descriptor.keys.filter(item => item.visible && entry.colors?.[item.id]);
    if (!keys.length) return node(doc, "span", undefined, "lux-thumb-fallback");
    const svg = doc.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.classList.add("lux-map"); svg.setAttribute("aria-hidden", "true");
    const left = Math.min(...keys.map(key => key.x)), top = Math.min(...keys.map(key => key.y));
    const right = Math.max(...keys.map(key => key.x + key.width));
    const bottom = Math.max(...keys.map(key => key.y + key.height));
    svg.setAttribute("viewBox", `${left} ${top} ${right - left} ${bottom - top}`);
    for (const key of keys) {
      const rect = doc.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("x", key.x + .05); rect.setAttribute("y", key.y + .05);
      rect.setAttribute("width", Math.max(.1, key.width - .1));
      rect.setAttribute("height", Math.max(.1, key.height - .1));
      rect.setAttribute("rx", ".08"); rect.setAttribute("fill", hex(entry.colors[key.id]));
      svg.append(rect);
    }
    return svg;
  }

  function renderLooks(parent, view) {
    if (view.picture?.capabilities?.backend_id !== "nia87" || view.descriptor?.backend_id !== "nia87") return;
    paintedKeys(view);
    const box = node(doc, "section", undefined, "lux-gallery");
    box.append(node(doc, "h3", "Looks"));
    const entries = catalog();
    const tiles = node(doc, "div", undefined, "lux-tiles");
    for (const entry of entries) {
      const tile = button(doc, "", () => selectLook(entry.id), !armed(view) || looks.applying, "lux-tile");
      tile.dataset.look = entry.id;
      tile.setAttribute("aria-pressed", String(looks.id === entry.id));
      tile.append(thumbnail(entry, view), node(doc, "span", entry.name, "lux-name"));
      tiles.append(tile);
    }
    box.append(tiles);
    if (!entries.length) box.append(node(doc, "p", "Looks unavailable.", "muted"));
    if (looks.id) {
      const row = node(doc, "div", undefined, "lux-look-options");
      const label = node(doc, "label", "Replace painted keys");
      const replace = node(doc, "input"); replace.type = "checkbox"; replace.checked = looks.replace;
      replace.disabled = looks.applying;
      replace.addEventListener("change", () => { looks.replace = replace.checked; looks.notice = replace.checked ? "Painted keys will be replaced." : "Painted keys kept."; redraw(); });
      label.prepend(replace); row.append(label);
      box.append(row);
    }
    parent.append(box);
  }

  function paintKey(key) {
    const view = runtime.view;
    if (!keyboardState(view).pictureMode || paintMode !== "Paint" || paintBlocked || !view.picture?.editor?.canEdit ||
        !view.picture.capabilities.keys.includes(key)) return false;
    if (looks.id) {
      if (stale(view) || !armed(view) || looks.applying) return false;
      looks.corrections[key] = rgb(paintColor);
      looks.notice = "Look preview changed. Keyboard unchanged.";
      redraw();
    } else {
      const target = runtime.identity(), selector = selectorContext(view);
      void runtime.edit("picture", { Color: { key, color: rgb(paintColor) } }, true).then(result => {
        if (result?.ok && runtime.isCurrent(target) && selectorContext(runtime.view) === selector) paintedKeys(runtime.view).add(key);
      });
    }
    return true;
  }

  function render(panel, view) {
    if (looks.connection && !runtime.isCurrent(looks.connection)) {
      looks.id = null; looks.corrections = {}; looks.replace = false; looks.target = null;
      looks.connection = null; looks.notice = "";
    }
    const state = view.lighting;
    panel.append(node(doc, "h2", "Lighting"));
    if (!state) return;
    const editor = state.editor, draft = editor.draft, caps = state.capabilities;
    panel.append(actions(doc, editor, "lighting", input => runtime.intent(input), { auto: true }));
    if (showHostLighting && runtime.hostModesAvailable) { host.render(panel, view); return; }
    if (!draft) {
      const content = editor.baseline?.content;
      panel.append(node(doc, "p", content?.HostActive
        ? "A host lighting mode is stored on the keyboard. Select an onboard lighting mode above to replace it."
        : content?.Opaque?.reason ?? "Read lighting to edit it.", "muted"));
      renderLooks(panel, view);
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
    renderLooks(panel, view);
    const pictureEffect = view.picture?.capabilities?.lighting_effect;
    if (pictureEffect && draft.effect === pictureEffect) {
      const pictureEditor = view.picture.editor;
      const colors = node(doc, "div", undefined, "card");
      colors.append(node(doc, "h3", "Per-key colors"));
      colors.append(node(doc, "p", "Choose a brush, then paint keys.", "muted"));
      colors.append(button(doc, "Load selected layer", () => void runtime.intent({ type: "preparePicture" }), view.connection.kind !== "connected" || view.busy || editor.dirty));
      const modes = node(doc, "div", undefined, "actions lux-paint-modes");
      for (const mode of ["Inspect", "Paint"]) {
        const control = button(doc, mode, () => { paintMode = mode; redraw(); }, false, paintMode === mode ? "primary" : "quiet");
        control.setAttribute("aria-pressed", String(paintMode === mode));
        modes.append(control);
      }
      colors.append(modes);
      const picker = node(doc, "input"); picker.type = "color"; picker.value = paintColor;
      picker.disabled = looks.applying || paintBlocked || !pictureEditor.canEdit;
      picker.addEventListener("input", () => { paintColor = picker.value; });
      picker.addEventListener("change", () => { paintColor = picker.value; redraw(); });
      field(doc, colors, "Brush color", picker);
      if (!looks.id) colors.append(actions(doc, pictureEditor, "picture", input => runtime.intent(input), { auto: true }));
      panel.append(colors);
    }
    if (looks.id) {
      const row = node(doc, "div", undefined, "actions lux-actions");
      row.append(button(doc, "Revert preview", () => void confirmDiscardLook(), looks.applying));
      row.append(button(doc, "Apply look", () => void applyLook(), looks.applying || stale(view) || !armed(view) || view.busy, "primary"));
      panel.append(row, node(doc, "p", stale(view) ? "Return to the original layer or discard this preview." :
        (looks.notice || "Preview only. Keyboard unchanged."), "muted lux-status"));
    } else if (looks.notice) panel.append(node(doc, "p", looks.notice, "muted lux-status"));
  }

  return { render, renderMode, keyboardState, paintKey, destroy: () => host.destroy(), stopHost: () => host.stop() };
}
