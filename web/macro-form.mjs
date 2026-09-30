// SPDX-License-Identifier: GPL-3.0-or-later
import { librarySlots } from "./macro_view.mjs";
import { node, button, field, choice, number, actions, range, same, text, download } from "./widgets.mjs";

function eventLabel(event) {
  const action = event.action;
  if (action.Key) return `Key ${action.Key.usage} ${action.Key.pressed ? "↓" : "↑"}`;
  if (action.Button) return `Button ${action.Button.button} ${action.Button.pressed ? "↓" : "↑"}`;
  if (action.Move) return `Move ${action.Move.dx}, ${action.Move.dy}`;
  if (action.Backend) return `${action.Backend.id} ${action.Backend.pressed ? "↓" : "↑"}`;
  return "Unknown event";
}

// Core owns macro values. This owner keeps only unsubmitted browser form fields and local names.
export function createMacroForm({ doc, win, runtime, storage, recorder, redraw, navigate, confirmDiscard }) {
  let eventIndex = null, eventType = "Key", eventUsage = 4, eventButton = 1;
  let eventDx = 0, eventDy = 0, eventDelay = 20, eventPressed = true, eventBackend = "", macroBinding = "counted";
  let formRevision = 0, importVersion = 0;
  const touch = () => { formRevision++; };
  const products = new Map(), bindingDrafts = new Map();
  let namesState = { baseline: {}, drafts: new Map(), saves: new Map() };
  function nameFor(slot) { return namesState.drafts.get(slot) ?? namesState.baseline[slot] ?? ""; }
  function connected(device, generation) {
    const product = `${device.vendorId}:${device.productId}`;
    if (!products.has(product)) products.set(product, { baseline: {}, drafts: new Map(), saves: new Map() });
    namesState = products.get(product);
    bindingDrafts.clear(); touch(); importVersion++;
    const target = {device, generation}, state = namesState, saves = new Map(state.saves);
    void storage.macroNames?.(device)?.then(names => {
      if (!runtime.isCurrent(target)) return;
      const loaded = { ...names };
      for (const [slot, version] of state.saves) {
        if (version === saves.get(slot)) continue;
        if (state.baseline[slot] !== undefined) loaded[slot] = state.baseline[slot];
        else delete loaded[slot];
      }
      state.baseline = loaded; redraw(true);
    }).catch(error => {
      if (runtime.isCurrent(target)) runtime.announce(`Local macro names could not be read: ${text(error)}`, true, true);
    });
  }
  function disconnected() {
    bindingDrafts.clear(); eventIndex = null; touch(); importVersion++;
    // Retain local name drafts by product, just as core retains unsaved feature drafts.
  }
  async function saveMacroName(slot) {
    const target = runtime.identity();
    if (!target.device) return;
    const name = nameFor(slot), state = namesState;
    try {
      if (typeof storage.saveMacroName !== "function") throw new Error("Local name storage is unavailable.");
      await storage.saveMacroName(target.device, slot, name);
      if (!runtime.isCurrent(target)) return;
      state.saves.set(slot, (state.saves.get(slot) ?? 0) + 1);
      if (name.trim()) state.baseline[slot] = name; else delete state.baseline[slot];
      if (state.drafts.get(slot) === name) state.drafts.delete(slot);
      runtime.announce("Local macro name saved. Keyboard configuration is unchanged.", false, true);
    } catch (error) {
      if (runtime.isCurrent(target)) runtime.announce(`Local macro name was not saved: ${text(error)}`, true, true);
    }
  }
  async function selectMacro(slot) {
    if (slot === runtime.view.macros?.slot) return;
    const target = runtime.identity(), previousSlot = runtime.view.macros?.slot;
    if (runtime.view.macros?.editor.dirty) {
      if (!await confirmDiscard() || !runtime.isCurrent(target) || runtime.view.macros?.slot !== previousSlot) return;
      await runtime.intent({ type: "revert", feature: "macro" });
    }
    if (!runtime.isCurrent(target)) return;
    const selected = await runtime.intent({ type: "selectMacro", slot });
    eventIndex = null; touch();
    if (selected.ok && runtime.isCurrent(target)) await runtime.intent({ type: "read", feature: "macro" });
  }

  async function newMacro() {
    const target = runtime.identity();
    const candidate = await runtime.intent({ type: "macroCandidate" });
    if (!candidate.ok || !runtime.isCurrent(target)) return;
    const slot = candidate.outcome.slot;
    await selectMacro(slot);
    if (!runtime.isCurrent(target)) return;
    if (runtime.view.macros?.slot === slot && !runtime.view.macros.editor.draft) await runtime.intent({ type: "read", feature: "macro" });
    const editor = runtime.view.macros?.editor;
    if (runtime.view.macros?.slot !== slot || !editor?.draft || editor.draft.events.length) {
      runtime.announce("The selected slot is not confirmed empty. Choose another slot.", true);
      return;
    }
    await runtime.intent({ type: "initializeMacro" });
    navigate("macros");
  }

  function selectedKeyWritable({ layer, key }) {
    return runtime.view.descriptor.keys.some(item => item.id === key && item.writable)
      && !runtime.view.descriptor.layers.find(item => item.id === layer)?.read_only_keys.includes(key);
  }

  function macroEvent() {
    const action = eventType === "Key" ? { Key: { usage: Number(eventUsage), pressed: eventPressed } }
      : eventType === "Button" ? { Button: { button: Number(eventButton), pressed: eventPressed } }
      : eventType === "Backend" ? { Backend: { backend_id: runtime.view.macros.capabilities.backend_id, id: eventBackend, pressed: eventPressed } }
      : { Move: { dx: Number(eventDx), dy: Number(eventDy) } };
    return { action, delay_ms: Number(eventDelay) };
  }

  function renderMacroForm(parent, editor, caps, countValid) {
    const form = node(doc, "div", undefined, "card form-grid");
    form.append(node(doc, "h3", eventIndex === null ? "Add event" : `Edit event ${eventIndex + 1}`));
    const types = [["Key", "Key"], ["Button", "Mouse button"], ["Move", "Pointer move"]];
    if (caps.backend_actions.length) types.push(["Backend", "Keyboard action"]);
    field(doc, form, "Type", choice(doc, types, eventType, value => { touch(); eventType = value; redraw(); }, !editor.canEdit));
    if (eventType === "Key") field(doc, form, "USB key usage", number(doc, eventUsage, caps.keys?.start ?? 4, caps.keys?.end ?? 239, value => { touch(); eventUsage = value; }, !editor.canEdit));
    if (eventType === "Button") field(doc, form, "Mouse button", choice(doc, caps.buttons.map(item => [item.button, item.label]), eventButton, value => { touch(); eventButton = Number(value); }, !editor.canEdit));
    if (eventType === "Backend") {
      if (!eventBackend) eventBackend = caps.backend_actions[0]?.id;
      field(doc, form, "Action", choice(doc, caps.backend_actions.map(item => [item.id, item.label]), eventBackend, value => { touch(); eventBackend = value; }, !editor.canEdit));
    }
    if (eventType === "Move") {
      const [min, max] = range(caps.movement);
      field(doc, form, "Horizontal", number(doc, eventDx, min, max, value => { touch(); eventDx = value; }, !editor.canEdit));
      field(doc, form, "Vertical", number(doc, eventDy, min, max, value => { touch(); eventDy = value; }, !editor.canEdit));
    } else {
      const label = node(doc, "label", "Press (release when unchecked)");
      const check = node(doc, "input"); check.type = "checkbox"; check.checked = eventPressed; check.disabled = !editor.canEdit;
      check.addEventListener("change", () => { touch(); eventPressed = check.checked; });
      label.prepend(check); form.append(label);
    }
    field(doc, form, "Wait after (ms)", number(doc, eventDelay, caps.delays_ms.start, caps.delays_ms.end, value => { touch(); eventDelay = value; }, !editor.canEdit));
    form.append(button(doc, eventIndex === null ? "Add event" : "Replace event", async () => {
      const change = eventIndex === null ? { Insert: { at: runtime.view.macros.editor.draft.events.length, event: macroEvent() } }
        : { Replace: { at: eventIndex, event: macroEvent() } };
      const result = await runtime.edit("macro", change);
      if (result.ok) { eventIndex = null; redraw(); }
    }, !editor.canEdit || !countValid, "primary"));
    if (eventIndex !== null) form.append(button(doc, "Cancel editing", () => { eventIndex = null; redraw(); }));
    parent.append(form);
  }

  function render(panel, view, { layer, key }) {
    const state = view.macros;
    panel.append(node(doc, "h2", "Macros"), node(doc, "p", "Save a sequence to a slot, then assign it to a key.", "muted"));
    if (!state) return;
    const editor = state.editor, caps = state.capabilities;
    const layout = node(doc, "div", undefined, "macro-layout");
    layout.addEventListener("input", touch);
    const library = node(doc, "div", undefined, "macro-library");
    library.append(node(doc, "h3", "Library"));
    library.append(button(doc, "New macro", () => void newMacro(), !view.keymap.draft || view.connection.kind !== "connected" || editor.dirty || view.recording));
    const list = node(doc, "div", undefined, "slot-list");
    const { visible: visibleSlots, bound } = librarySlots(caps, state.occupancy, view.keymap.draft, state.slot);
    for (const slot of visibleSlots) {
      const label = nameFor(slot.id).trim() || slot.label;
      list.append(button(doc, `${label} · ${state.occupancy[slot.id] ?? "Unknown"}${bound.has(slot.id) ? " · bound" : ""}`, () => void selectMacro(slot.id), view.recording, slot.id === state.slot ? "active" : ""));
    }
    library.append(list);
    if (state.discoveryBusy) library.append(node(doc, "p", "Discovering slot occupancy…", "muted"));
    if (state.discoveryError) {
      library.append(node(doc, "p", state.discoveryError, "error"));
      library.append(button(doc, "Retry discovery", () => void runtime.intent({ type: "discoverMacros" }), state.discoveryBusy));
    } else if (!state.discoveryBusy) {
      const used = Object.values(state.occupancy).filter(value => value === "Configured" || value === "Opaque").length;
      library.append(node(doc, "p", `${used} of ${caps.slots.length} slots used`, "muted"));
    }
    const detail = node(doc, "div", undefined, "macro-detail");
    detail.append(node(doc, "h3", `${nameFor(state.slot).trim() || caps.slots.find(item => item.id === state.slot)?.label || state.slot} · Editor`));
    detail.append(actions(doc, editor, "macro", input => runtime.intent(input), { revert: async () => {
      const target = runtime.identity(), slot = runtime.view.macros?.slot;
      if (runtime.view.macros?.editor.dirty && !await confirmDiscard()) return;
      if (!runtime.isCurrent(target) || runtime.view.macros?.slot !== slot) return;
      await runtime.intent({ type: "revert", feature: "macro" });
    } }));
    if (editor.baseline) {
      const nameRow = node(doc, "div", undefined, "name-row");
      const nameInput = node(doc, "input");
      nameInput.type = "text";
      nameInput.placeholder = "Local macro name";
      nameInput.value = nameFor(state.slot);
      nameInput.disabled = view.recording;
      nameInput.addEventListener("input", () => { touch(); namesState.drafts.set(state.slot, nameInput.value); });
      nameRow.append(nameInput, button(doc, "Save name", () => void saveMacroName(state.slot), !runtime.connection.device || view.recording));
      detail.append(nameRow);
    }
    if (!editor.draft) {
      detail.append(node(doc, "p", "Read this slot before editing. Unknown slots are not treated as empty.", "muted"));
      detail.append(button(doc, "Read selected slot", () => void runtime.intent({ type: "read", feature: "macro" }), !editor.canRead));
    } else {
      const playback = node(doc, "div", undefined, "card");
      playback.append(node(doc, "h3", "Playback"));
      const countValid = editor.draft.repeat_count >= caps.editable_repeat_counts.start
        && editor.draft.repeat_count <= caps.editable_repeat_counts.end;
      const needsInitialization = !countValid && editor.draft.events.length === 0 && !editor.dirty && editor.status === "Ready";
      if (needsInitialization) playback.append(button(doc, "Initialize empty macro", () => void runtime.intent({ type: "initializeMacro" }), !editor.canEdit, "primary"));
      field(doc, playback, "Repeat count", number(doc, editor.draft.repeat_count, caps.editable_repeat_counts.start, caps.editable_repeat_counts.end, value => void runtime.edit("macro", { Repeat: value }), !editor.canEdit));
      recorder.renderControls(detail, caps, editor, countValid);
      const events = node(doc, "div", undefined, "event-list");
      editor.draft.events.forEach((event, index) => {
        const row = node(doc, "div", undefined, "event-row");
        row.append(node(doc, "span", `${index + 1}. ${eventLabel(event)} · ${event.delay_ms} ms`));
        row.append(button(doc, "Edit", () => {
          touch(); eventIndex = index; eventDelay = event.delay_ms;
          eventType = Object.keys(event.action)[0];
          if (event.action.Key) { eventUsage = event.action.Key.usage; eventPressed = event.action.Key.pressed; }
          if (event.action.Button) { eventButton = event.action.Button.button; eventPressed = event.action.Button.pressed; }
          if (event.action.Move) { eventDx = event.action.Move.dx; eventDy = event.action.Move.dy; }
          if (event.action.Backend) { eventBackend = event.action.Backend.id; eventPressed = event.action.Backend.pressed; }
          redraw();
        }, !editor.canEdit));
        row.append(button(doc, "↑", () => void runtime.edit("macro", { Move: { from: index, to: index - 1 } }), !editor.canEdit || index === 0));
        row.append(button(doc, "↓", () => void runtime.edit("macro", { Move: { from: index, to: index + 1 } }), !editor.canEdit || index === editor.draft.events.length - 1));
        row.append(button(doc, "Remove", () => void runtime.edit("macro", { Remove: { at: index } }), !editor.canEdit));
        events.append(row);
      });
      detail.append(events);
      renderMacroForm(detail, editor, caps, countValid);
      const assign = playback;
      assign.append(node(doc, "h3", "Save and assign"), node(doc, "p", `Target: ${view.descriptor.keys.find(item => item.id === key)?.label ?? "key"} · ${view.descriptor.layers.find(item => item.id === layer)?.label ?? "layer"}`, "muted"));
      const bindings = caps.bindings.filter(item => item.slot === state.slot);
      const assigned = view.keymap.draft?.[layer]?.[key];
      const currentBinding = bindings.find(item => same(item.action, assigned))?.id;
      macroBinding = bindingDrafts.get(state.slot) ?? currentBinding ?? bindings[0]?.id;
      field(doc, assign, "Playback mode", choice(doc, bindings.map(item => [item.id, item.label]), macroBinding, value => { touch(); macroBinding = value; bindingDrafts.set(state.slot, value); }, !editor.canEdit));
      assign.append(button(doc, "Save and assign to selected key", () => void runtime.intent({ type: "saveAndAssignMacro", layer, key, binding: macroBinding }), editor.status !== "Ready" || !countValid || view.busy || !selectedKeyWritable({layer,key}), "primary"));
      library.append(playback);
      const documents = node(doc, "details", undefined, "card");
      documents.append(node(doc, "summary", "Import or export macro JSON"));
      const file = node(doc, "input"); file.type = "file"; file.accept = "application/json,.json";
      file.disabled = view.recording;
      file.addEventListener("change", async () => {
        if (!file.files?.[0]) return;
        const target = runtime.identity(), slot = state.slot, version = ++importVersion;
        const revision = formRevision;
        const draft = JSON.stringify(runtime.view.macros?.editor.draft);
        const currentTarget = () => runtime.isCurrent(target) && runtime.view.macros?.slot === slot && version === importVersion;
        const unchanged = () => formRevision === revision && JSON.stringify(runtime.view.macros?.editor.draft) === draft;
        try {
          if (file.files[0].size > 64 * 1024) throw new Error("Macro JSON exceeds the 64 KiB import limit.");
          const contents = JSON.parse(await file.files[0].text());
          if (!currentTarget()) return;
          if (!unchanged()) {
            runtime.announce("Import skipped because the macro or its local fields changed while the file was loading.", false, true);
            return;
          }
          const result = await runtime.intent({ type: "importMacroDocument", document: contents });
          if (!currentTarget() || formRevision !== revision) return;
          if (result.ok) {
            namesState.drafts.set(slot, result.outcome.name);
            if (contents.backend_id === runtime.view.descriptor.backend_id && caps.bindings.some(binding => binding.slot === slot && binding.id === result.outcome.binding)) {
              macroBinding = result.outcome.binding; bindingDrafts.set(slot, macroBinding);
            } else bindingDrafts.delete(slot);
            touch(); redraw();
          }
        } catch (error) {
          if (currentTarget() && unchanged()) runtime.announce(`Could not read macro file: ${text(error)}`, true, true);
        }
      });
      documents.append(file);
      documents.append(button(doc, "Export macro", async () => {
        const result = await runtime.intent({ type: "exportMacroDocument", name: nameFor(state.slot), binding: bindingDrafts.get(state.slot) ?? null });
        if (result.ok) download(doc, win, `${state.slot}.json`, JSON.stringify(result.outcome.document, null, 2));
      }, view.recording));
      detail.append(documents);
    }
    layout.append(detail, library);
    panel.append(layout);
  }

  return {
    render, connected, disconnected,
    get names() { return { ...namesState.baseline, ...Object.fromEntries(namesState.drafts) }; },
    get hasUnsavedNames() {
      return [...products.values()].some(state => [...state.drafts].some(([slot, name]) => name !== (state.baseline[slot] ?? "")));
    },
  };
}
