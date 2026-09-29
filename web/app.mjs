// SPDX-License-Identifier: GPL-3.0-or-later
import { filters, selectDevice } from "./transport.mjs";
import { DeviceExecutor } from "./executor.mjs";
import { recordedKey } from "./input.mjs";

const text = error => String(error?.message ?? error);
const hex = rgb => `#${rgb.map(byte => byte.toString(16).padStart(2, "0")).join("")}`;
const rgb = color => [1, 3, 5].map(at => Number.parseInt(color.slice(at, at + 2), 16));
const range = value => value ? [value.start, value.end] : [0, 0];
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const editable = entry => entry?.editor ?? entry;

function node(doc, tag, label, className) {
  const element = doc.createElement(tag);
  if (label !== undefined) element.textContent = label;
  if (className) element.className = className;
  return element;
}

function button(doc, label, action, disabled = false, className = "") {
  const element = node(doc, "button", label, className);
  element.type = "button";
  element.disabled = disabled;
  element.addEventListener("click", action);
  return element;
}

function field(doc, parent, label, control) {
  const wrapper = node(doc, "label", undefined, "field");
  wrapper.append(node(doc, "span", label), control);
  parent.append(wrapper);
  return control;
}

function choice(doc, options, current, changed, disabled = false) {
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

function number(doc, value, min, max, changed, disabled = false) {
  const input = node(doc, "input");
  input.type = "number";
  input.value = String(value ?? min);
  input.min = String(min);
  input.max = String(max);
  input.disabled = disabled;
  input.addEventListener("change", () => changed(Number(input.value)));
  return input;
}

function actionLabel(action, catalog) {
  if (!action) return "—";
  const known = catalog.find(item => same(item.action, action));
  if (known) return known.label;
  if (action.Opaque) return action.Opaque.label || "Unknown binding";
  if (action.Shortcut) return "Shortcut";
  if (action.Macro) return `Macro ${Number(action.Macro.slot) + 1}`;
  return Object.keys(action)[0] ?? "Unknown";
}

function eventLabel(event) {
  const action = event.action;
  if (action.Key) return `Key ${action.Key.usage} ${action.Key.pressed ? "↓" : "↑"}`;
  if (action.Button) return `Button ${action.Button.button} ${action.Button.pressed ? "↓" : "↑"}`;
  if (action.Move) return `Move ${action.Move.dx}, ${action.Move.dy}`;
  if (action.Backend) return `${action.Backend.id} ${action.Backend.pressed ? "↓" : "↑"}`;
  return "Unknown event";
}

export function mount(root, { codec, hid, storage, now = () => performance.now(), wait } = {}) {
  if (!root || !codec?.BrowserSession || !codec?.BrowserOperation || !hid || !storage) {
    throw new Error("The browser session, WebHID and backup storage are required.");
  }
  const doc = root.ownerDocument;
  const win = doc.defaultView ?? globalThis;
  const session = new codec.BrowserSession();
  let view = JSON.parse(session.view());
  let device = null, executor = null, connecting = false, closing = false;
  let tab = "keymap", layer = view.descriptor.layers[0]?.id, key = view.descriptor.keys[0]?.id;
  let notice = "Choose your Nia87 to load its keymap and lighting.";
  let writes = 0, disposed = false, previousDiscovery = false;
  const inflight = new Set();
  let destroying = null;
  let eventIndex = null, eventType = "Key", eventUsage = 4, eventButton = 1;
  let eventDx = 0, eventDy = 0, eventDelay = 20, eventPressed = true, eventBackend = "";
  let shortcutKey = view.descriptor.shortcuts?.keys[0]?.usage ?? 4;
  const shortcutMods = new Set();
  let paintColor = "#ffffff", paintReady = false, macroBinding = "counted";
  const timers = new Map();

  root.replaceChildren();
  const shell = node(doc, "div", undefined, "shell");
  const header = node(doc, "header", undefined, "topbar");
  const brand = node(doc, "div", undefined, "brand");
  brand.append(node(doc, "strong", "BYAKKO"), node(doc, "small", "NIA87 · WEBHID"));
  const connection = node(doc, "div", undefined, "connection");
  const workspace = node(doc, "div", undefined, "workspace");
  const sidebar = node(doc, "nav", undefined, "sidebar");
  sidebar.setAttribute("aria-label", "Features");
  const main = node(doc, "div", undefined, "main-content");
  const status = node(doc, "p", undefined, "status");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  const keyboard = node(doc, "div", undefined, "keyboard-area");
  const panel = node(doc, "section", undefined, "panel");
  const dialog = node(doc, "dialog", undefined, "discard-dialog");
  dialog.append(node(doc, "h2", "Discard macro edits?"), node(doc, "p", "The current macro draft has unsaved changes."));
  const dialogActions = node(doc, "div", undefined, "actions");
  dialog.append(dialogActions);
  const backupDialog = node(doc, "dialog", undefined, "backup-dialog");
  backupDialog.append(
    node(doc, "h2", "Diagnostic backups"),
    node(doc, "p", "These are the exact saved records. Copy the JSON or request a download to preserve it outside this browser.", "muted"),
  );
  const backupText = node(doc, "textarea");
  backupText.readOnly = true;
  backupText.setAttribute("aria-label", "Backup JSON");
  backupDialog.append(backupText);
  const backupActions = node(doc, "div", undefined, "actions");
  backupDialog.append(backupActions);
  header.append(brand, connection);
  main.append(status, keyboard, panel);
  workspace.append(sidebar, main);
  shell.append(header, workspace, dialog, backupDialog);
  root.append(shell);

  function confirmDiscard() {
    if (typeof dialog.showModal !== "function") return Promise.resolve(Boolean(win.confirm?.("Discard unsaved macro edits?")));
    return new Promise(resolve => {
      dialogActions.replaceChildren();
      dialogActions.append(
        button(doc, "Keep editing", () => { dialog.close(); resolve(false); }),
        button(doc, "Discard changes", () => { dialog.close(); resolve(true); }, false, "danger"),
      );
      dialog.showModal();
    });
  }

  function handle(response) {
    view = response.view;
    executor?.setRecording(view.recording);
    const discovery = Boolean(view.macros?.discoveryBusy);
    if (previousDiscovery && !discovery) executor?.cancelCatalog();
    previousDiscovery = discovery;
    if (!response.ok) notice = response.error;
    else if (response.outcome?.kind === "failed" || response.outcome?.kind?.endsWith("Failed")) {
      notice = JSON.stringify(response.outcome.detail ?? response.outcome.kind);
    } else if (response.outcome?.kind === "conflict") notice = "The keyboard changed. Review and read this feature again.";
    else {
      const saved = {
        saved: "Keymap saved and verified by keyboard readback.",
        macroSaved: "Macro saved and verified by keyboard readback.",
        lightingSaved: "Lighting accepted by the keyboard after pacing. It was not read back.",
        pictureSaved: "Per-key colors accepted by the keyboard after pacing. They were not read back.",
        settingsSaved: "Settings saved and verified by keyboard readback.",
        assignmentSucceeded: "Macro saved and assigned; the keymap was verified by readback.",
      };
      if (saved[response.outcome?.kind]) notice = saved[response.outcome.kind];
    }
    render();
    return response;
  }

  async function execute(command) {
    let last = null;
    while (command) {
      if (!executor) throw new Error("No selected keyboard is connected.");
      const write = Boolean(Object.values(command.payload)[0]?.Apply);
      if (write) writes++;
      render();
      try {
        const completion = await executor.run(command);
        if (disposed) { command = null; continue; }
        const result = handle(JSON.parse(session.accept(JSON.stringify(completion))));
        last = result;
        command = result.command;
      } finally {
        if (write) writes--;
        render();
      }
    }
    return last;
  }

  async function intent(input, background = false) {
    const result = handle(JSON.parse(session.dispatch(JSON.stringify(input))));
    if (result.ok && !result.command) {
      const messages = {
        edit: "Draft updated. Save to keyboard when ready.",
        revert: "Draft reverted.",
        initializeMacro: "Empty macro initialized. Add events, then save.",
        importMacroDocument: "Macro document staged. Save to keyboard when ready.",
        recordStart: "Recording locally. Press keys, then stop to release held inputs.",
        recordStop: "Recording stopped. Review events before saving.",
      };
      if (messages[input.type]) { notice = messages[input.type]; render(); }
    }
    if (result.command) {
      const running = execute(result.command).catch(error => {
        notice = text(error);
        render();
        return { ok: false, error: notice, view };
      });
      inflight.add(running);
      void running.finally(() => inflight.delete(running));
      if (!background) return await running;
    }
    return result;
  }

  function schedule(feature) {
    win.clearTimeout(timers.get(feature));
    const attempt = async () => {
      timers.delete(feature);
      const editor = editable(view[feature]);
      if (!editor?.dirty || !device) return;
      if (editor.status !== "Ready" || view.connection.kind !== "connected") return;
      if (!editor.canApply) {
        timers.set(feature, win.setTimeout(attempt, 250));
        return;
      }
      await intent({ type: "apply", feature });
    };
    timers.set(feature, win.setTimeout(attempt, 500));
  }

  async function edit(feature, change, autosave = false) {
    const result = await intent({ type: "edit", feature, change });
    if (result.ok && autosave) schedule(feature);
    return result;
  }

  async function connect() {
    if (device || connecting || closing) return;
    connecting = true;
    render();
    try {
      // Permission must start synchronously inside this click handler.
      const candidates = await hid.requestDevice({ filters });
      if (!candidates.length) { notice = "Keyboard selection was cancelled."; return; }
      device = selectDevice(candidates);
      executor = new DeviceExecutor(device, codec.BrowserOperation, storage, wait);
      const result = handle(JSON.parse(session.dispatch('{"type":"connect"}')));
      if (!result.ok) throw new Error(result.error);
      notice = `Reading ${device.productName || "Nia87"}…`;
      render();
      await intent({ type: "read", feature: "keymap" });
      await intent({ type: "read", feature: "lighting" });
      notice = view.keymap.status === "Ready" && view.lighting?.editor.status === "Ready"
        ? "Keyboard ready. Select a key or feature to edit."
        : "Some keyboard data could not be read. Use the feature Read control to retry.";
      render();
      void intent({ type: "discoverMacros" }, true);
    } catch (error) {
      notice = `Could not open the keyboard: ${text(error)}`;
      if (device) await disconnect(true);
    } finally {
      connecting = false;
      render();
    }
  }

  async function disconnect(force = false) {
    if (!device || closing || (writes && !force)) return;
    closing = true;
    for (const timer of timers.values()) win.clearTimeout(timer);
    timers.clear();
    handle(JSON.parse(session.dispatch('{"type":"disconnect"}')));
    const old = executor;
    executor = null;
    device = null;
    if (!force) notice = "Disconnected. Choose a keyboard to reconnect.";
    try { await old?.close(); }
    catch (error) { notice = `Device access ended: ${text(error)}`; }
    finally { closing = false; render(); }
  }

  async function selectMacro(slot) {
    if (slot === view.macros?.slot) return;
    if (view.macros?.editor.dirty) {
      if (!await confirmDiscard()) return;
      await intent({ type: "revert", feature: "macro" });
    }
    const selected = await intent({ type: "selectMacro", slot });
    eventIndex = null;
    if (selected.ok) await intent({ type: "read", feature: "macro" });
  }

  async function newMacro() {
    const candidate = await intent({ type: "macroCandidate" });
    if (!candidate.ok) return;
    const slot = candidate.outcome.slot;
    await selectMacro(slot);
    if (view.macros?.slot === slot && !view.macros.editor.draft) await intent({ type: "read", feature: "macro" });
    const editor = view.macros?.editor;
    if (view.macros?.slot !== slot || !editor?.draft || editor.draft.events.length) {
      notice = "The selected slot is not confirmed empty. Choose another slot.";
      render();
      return;
    }
    await intent({ type: "initializeMacro" });
    tab = "macros";
    render();
  }

  function renderConnection() {
    connection.replaceChildren();
    connection.append(node(doc, "span", device ? `${device.productName || "Nia87"} · ${device.vendorId.toString(16)}:${device.productId.toString(16)}` : "No keyboard selected", "device-name"));
    connection.append(button(doc, device ? "Connected" : "Choose keyboard", connect, Boolean(device || connecting || closing), "primary"));
    if (device) connection.append(button(doc, "Disconnect", () => void disconnect(), Boolean(writes || closing)));
  }

  function renderSidebar() {
    sidebar.replaceChildren(node(doc, "p", "WORKSPACE", "eyebrow"));
    for (const [id, label] of [["keymap", "Keymap"], ["macros", "Macros"], ["lighting", "Lighting"]]) {
      const dirty = editable(view[id])?.dirty;
      const item = button(doc, `${label}${dirty ? " •" : ""}`, () => { tab = id; render(); }, false, tab === id ? "nav-item active" : "nav-item");
      item.setAttribute("aria-current", tab === id ? "page" : "false");
      sidebar.append(item);
    }
    sidebar.append(node(doc, "span", "Settings · later", "nav-disabled"));
    const tools = node(doc, "div", undefined, "sidebar-tools");
    tools.append(button(doc, "View backups", async () => {
      try {
        const data = await storage.all();
        const contents = JSON.stringify(data, null, 2);
        backupText.value = contents;
        backupActions.replaceChildren(
          button(doc, "Download JSON", () => {
            download("byakko-backups.json", contents);
            notice = "Backup download requested. The JSON remains visible here for copying.";
            render();
          }, data.length === 0, "primary"),
          button(doc, "Close", () => {
            if (typeof backupDialog.close === "function") backupDialog.close();
            else backupDialog.removeAttribute("open");
          }),
        );
        if (typeof backupDialog.showModal === "function") backupDialog.showModal();
        else backupDialog.setAttribute("open", "");
        notice = `Prepared ${data.length} diagnostic backup records for review.`;
      } catch (error) { notice = `Backup export failed: ${text(error)}`; }
      render();
    }));
    sidebar.append(tools);
  }

  function download(filename, content) {
    const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
    const anchor = node(doc, "a");
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    win.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function renderKeyboard() {
    keyboard.replaceChildren();
    const heading = node(doc, "div", undefined, "keyboard-heading");
    heading.append(node(doc, "div", `${view.descriptor.device_name} · ${tab === "lighting" ? "Per-key colors" : tab === "macros" ? "Assignment target" : "Key bindings"}`));
    if (tab !== "lighting") {
      const layers = node(doc, "div", undefined, "layer-tabs");
      for (const entry of view.descriptor.layers) layers.append(button(doc, entry.label, () => { layer = entry.id; render(); }, false, entry.id === layer ? "active" : ""));
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
      const element = button(doc, item.label, () => {
        key = item.id;
        if (tab === "lighting" && paintReady && view.picture?.editor?.canEdit && view.picture.capabilities.keys.includes(key)) {
          void edit("picture", { Color: { key, color: rgb(paintColor) } }, true);
        } else render();
      }, false, `key ${key === item.id ? "selected" : ""} ${view.descriptor.layers.find(entry => entry.id === layer)?.read_only_keys.includes(item.id) || !item.writable ? "protected" : ""}`);
      element.title = tab === "keymap" ? actionLabel(view.keymap.draft?.[layer]?.[item.id], view.descriptor.actions) : item.label;
      element.style.left = `${item.x * 43 + 7}px`;
      element.style.top = `${item.y * 43 + 7}px`;
      element.style.width = `${item.width * 43 - 4}px`;
      element.style.height = `${item.height * 43 - 4}px`;
      if (tab === "lighting" && colors[item.id]) element.style.setProperty("--key-color", hex(colors[item.id]));
      board.append(element);
    }
    scroll.append(board);
    keyboard.append(heading, scroll);
  }

  function actions(editor, feature, auto = false) {
    const row = node(doc, "div", undefined, "actions");
    row.append(button(doc, "Read", () => void intent({ type: "read", feature }), !editor?.canRead));
    row.append(button(doc, "Revert", async () => {
      if (feature === "macro" && editor?.dirty && !await confirmDiscard()) return;
      await intent({ type: "revert", feature });
    }, !editor?.canRevert));
    row.append(button(doc, "Save", () => void intent({ type: "apply", feature }), !editor?.canApply, "primary"));
    if (auto) row.append(node(doc, "span", "Changes save after 500 ms", "muted"));
    return row;
  }

  function renderKeymap() {
    const editor = view.keymap;
    panel.append(node(doc, "h2", "Keymap"), node(doc, "p", "Select a key, choose an action, then save.", "muted"));
    panel.append(actions(editor, "keymap"));
    const selected = view.descriptor.keys.find(item => item.id === key);
    const layerData = view.descriptor.layers.find(item => item.id === layer);
    const protectedKey = !selected?.writable || layerData?.read_only_keys.includes(key);
    const card = node(doc, "div", undefined, "card");
    card.append(node(doc, "h3", `${selected?.label ?? "Key"} · ${layerData?.label ?? "Layer"}`));
    card.append(node(doc, "p", protectedKey ? "Reserved for an onboard command" : `Current: ${actionLabel(editor.draft?.[layer]?.[key], view.descriptor.actions)}`, "muted"));
    const select = node(doc, "select");
    select.disabled = !editor.canEdit || protectedKey;
    const placeholder = node(doc, "option", "Choose action…");
    placeholder.value = "";
    select.append(placeholder);
    const groups = new Map();
    view.descriptor.actions.forEach((item, index) => {
      const category = item.category || "Other";
      let container = groups.get(category);
      if (!container) { container = node(doc, "optgroup"); container.label = category; groups.set(category, container); select.append(container); }
      const option = node(doc, "option", item.label);
      option.value = String(index);
      container.append(option);
    });
    select.value = "";
    select.addEventListener("change", () => {
      const action = view.descriptor.actions[Number(select.value)]?.action;
      if (action) void edit("keymap", { layer, key, action });
    });
    field(doc, card, "Action", select);
    const shortcuts = view.descriptor.shortcuts;
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
      box.append(button(doc, "Assign shortcut", () => void edit("keymap", { layer, key, action: { Shortcut: { modifiers: [...shortcutMods], key: shortcutKey } } }), !editor.canEdit));
      card.append(box);
    }
    panel.append(card);
  }

  function renderLighting() {
    const state = view.lighting;
    panel.append(node(doc, "h2", "Lighting"), node(doc, "p", "Onboard effects, brightness and per-key RGB.", "muted"));
    if (!state) return;
    const editor = state.editor, draft = editor.draft, caps = state.capabilities;
    panel.append(actions(editor, "lighting", true));
    if (!draft) { panel.append(node(doc, "p", "Read lighting to edit it.", "muted")); return; }
    const effect = caps.effects.find(item => item.id === draft.effect);
    const card = node(doc, "div", undefined, "card form-grid");
    field(doc, card, "Effect", choice(doc, caps.effects.map(item => [item.id, item.label]), draft.effect, value => void edit("lighting", { Effect: value }, true), !editor.canEdit));
    if (effect?.options.length) field(doc, card, "Option / per-key layer", choice(doc, effect.options.map(item => [item.id, item.label]), draft.option, value => void edit("lighting", { Option: value }, true), !editor.canEdit));
    for (const [name, property] of [["Brightness", "brightness"], ["Speed", "speed"]]) {
      if (!effect?.[property]) continue;
      const [min, max] = range(effect[property]);
      field(doc, card, name, number(doc, draft[property], min, max, value => void edit("lighting", { [name]: value }, true), !editor.canEdit));
    }
    if (effect?.color) {
      const fixed = draft.color?.Rgb;
      const color = node(doc, "input"); color.type = "color"; color.value = fixed ? hex(fixed) : "#ffffff";
      color.disabled = !editor.canEdit || draft.color === "Rainbow";
      color.addEventListener("input", () => { paintColor = color.value; });
      color.addEventListener("change", () => void edit("lighting", { Color: { Rgb: rgb(color.value) } }, true));
      field(doc, card, "Color", color);
      if (effect.color === "FixedOrRainbow" || effect.color === "Rainbow") {
        const label = node(doc, "label", "Rainbow");
        const checkbox = node(doc, "input"); checkbox.type = "checkbox"; checkbox.checked = draft.color === "Rainbow";
        checkbox.disabled = !editor.canEdit;
        checkbox.addEventListener("change", () => void edit("lighting", { Color: checkbox.checked ? "Rainbow" : { Rgb: rgb(color.value) } }, true));
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
      colors.append(button(doc, "Load selected layer", () => void intent({ type: "preparePicture" }), view.connection.kind !== "connected" || view.busy || editor.dirty));
      const picked = pictureEditor.draft?.[key];
      const picker = node(doc, "input"); picker.type = "color"; picker.value = paintReady ? paintColor : picked ? hex(picked) : paintColor;
      picker.disabled = !pictureEditor.canEdit || !view.picture.capabilities.keys.includes(key);
      picker.addEventListener("input", () => { paintColor = picker.value; });
      picker.addEventListener("change", () => { paintReady = true; void edit("picture", { Color: { key, color: rgb(picker.value) } }, true); });
      field(doc, colors, `Paint color · ${view.descriptor.keys.find(item => item.id === key)?.label ?? "key"}`, picker);
      if (paintReady) colors.append(button(doc, "Stop painting", () => { paintReady = false; render(); }));
      colors.append(actions(pictureEditor, "picture", true));
      panel.append(colors);
    }
  }

  function selectedKeyWritable() {
    return view.descriptor.keys.some(item => item.id === key && item.writable)
      && !view.descriptor.layers.find(item => item.id === layer)?.read_only_keys.includes(key);
  }

  function macroEvent() {
    const action = eventType === "Key" ? { Key: { usage: Number(eventUsage), pressed: eventPressed } }
      : eventType === "Button" ? { Button: { button: Number(eventButton), pressed: eventPressed } }
      : eventType === "Backend" ? { Backend: { backend_id: view.macros.capabilities.backend_id, id: eventBackend, pressed: eventPressed } }
      : { Move: { dx: Number(eventDx), dy: Number(eventDy) } };
    return { action, delay_ms: Number(eventDelay) };
  }

  function renderMacroForm(parent, editor, caps, countValid) {
    const form = node(doc, "div", undefined, "card form-grid");
    form.append(node(doc, "h3", eventIndex === null ? "Add event" : `Edit event ${eventIndex + 1}`));
    const types = [["Key", "Key"], ["Button", "Mouse button"], ["Move", "Pointer move"]];
    if (caps.backend_actions.length) types.push(["Backend", "Keyboard action"]);
    field(doc, form, "Type", choice(doc, types, eventType, value => { eventType = value; render(); }, !editor.canEdit));
    if (eventType === "Key") field(doc, form, "USB key usage", number(doc, eventUsage, caps.keys?.start ?? 4, caps.keys?.end ?? 239, value => { eventUsage = value; }, !editor.canEdit));
    if (eventType === "Button") field(doc, form, "Mouse button", choice(doc, caps.buttons.map(item => [item.button, item.label]), eventButton, value => { eventButton = Number(value); }, !editor.canEdit));
    if (eventType === "Backend") {
      if (!eventBackend) eventBackend = caps.backend_actions[0]?.id;
      field(doc, form, "Action", choice(doc, caps.backend_actions.map(item => [item.id, item.label]), eventBackend, value => { eventBackend = value; }, !editor.canEdit));
    }
    if (eventType === "Move") {
      const [min, max] = range(caps.movement);
      field(doc, form, "Horizontal", number(doc, eventDx, min, max, value => { eventDx = value; }, !editor.canEdit));
      field(doc, form, "Vertical", number(doc, eventDy, min, max, value => { eventDy = value; }, !editor.canEdit));
    } else {
      const label = node(doc, "label", "Press (release when unchecked)");
      const check = node(doc, "input"); check.type = "checkbox"; check.checked = eventPressed; check.disabled = !editor.canEdit;
      check.addEventListener("change", () => { eventPressed = check.checked; });
      label.prepend(check); form.append(label);
    }
    field(doc, form, "Wait after (ms)", number(doc, eventDelay, caps.delays_ms.start, caps.delays_ms.end, value => { eventDelay = value; }, !editor.canEdit));
    form.append(button(doc, eventIndex === null ? "Add event" : "Replace event", async () => {
      const change = eventIndex === null ? { Insert: { at: editor.draft.events.length, event: macroEvent() } }
        : { Replace: { at: eventIndex, event: macroEvent() } };
      const result = await edit("macro", change);
      if (result.ok) { eventIndex = null; render(); }
    }, !editor.canEdit || !countValid, "primary"));
    if (eventIndex !== null) form.append(button(doc, "Cancel editing", () => { eventIndex = null; render(); }));
    parent.append(form);
  }

  function renderMacros() {
    const state = view.macros;
    panel.append(node(doc, "h2", "Macros"), node(doc, "p", "Save a sequence to a slot, then assign it to a key.", "muted"));
    if (!state) return;
    const editor = state.editor, caps = state.capabilities;
    const layout = node(doc, "div", undefined, "macro-layout");
    const library = node(doc, "div", undefined, "macro-library");
    library.append(node(doc, "h3", "Library"));
    library.append(button(doc, "New macro", () => void newMacro(), !view.keymap.draft || !view.connection || view.connection.kind !== "connected"));
    const list = node(doc, "div", undefined, "slot-list");
    for (const slot of caps.slots) list.append(button(doc, `${slot.label} · ${state.occupancy[slot.id] ?? "Unknown"}`, () => void selectMacro(slot.id), false, slot.id === state.slot ? "active" : ""));
    library.append(list);
    if (state.discoveryBusy) library.append(node(doc, "p", "Discovering slot occupancy…", "muted"));
    if (state.discoveryError) library.append(node(doc, "p", state.discoveryError, "error"));
    const detail = node(doc, "div", undefined, "macro-detail");
    detail.append(node(doc, "h3", `${caps.slots.find(item => item.id === state.slot)?.label ?? state.slot} · Playback`));
    detail.append(actions(editor, "macro"));
    if (!editor.draft) {
      detail.append(node(doc, "p", "Read this slot before editing. Unknown slots are not treated as empty.", "muted"));
      detail.append(button(doc, "Read selected slot", () => void intent({ type: "read", feature: "macro" }), !editor.canRead));
    } else {
      const playback = node(doc, "div", undefined, "card");
      playback.append(node(doc, "h3", "Playback"));
      const countValid = editor.draft.repeat_count >= caps.editable_repeat_counts.start
        && editor.draft.repeat_count <= caps.editable_repeat_counts.end;
      const needsInitialization = !countValid && editor.draft.events.length === 0 && !editor.dirty && editor.status === "Ready";
      if (needsInitialization) playback.append(button(doc, "Initialize empty macro", () => void intent({ type: "initializeMacro" }), !editor.canEdit, "primary"));
      field(doc, playback, "Repeat count", number(doc, editor.draft.repeat_count, caps.editable_repeat_counts.start, caps.editable_repeat_counts.end, value => void edit("macro", { Repeat: value }), !editor.canEdit));
      const recordRow = node(doc, "div", undefined, "actions");
      recordRow.append(button(doc, view.recording ? "Stop recording" : "Record keys", async () => {
        if (view.recording) await intent({ type: "recordStop", nowMs: Math.round(now()) });
        else { await intent({ type: "recordStart", policy: { Measured: { terminal_ms: 0 } } }); doc.getElementById("record-capture")?.focus(); }
      }, !view.recording && (!editor.canEdit || !countValid)));
      const capture = node(doc, "div", view.recording ? "Recording keyboard input. Press keys here; stop to release held keys." : "Recording is local until you save.", "record-capture");
      capture.id = "record-capture"; capture.tabIndex = 0;
      detail.append(recordRow, capture);
      const events = node(doc, "div", undefined, "event-list");
      editor.draft.events.forEach((event, index) => {
        const row = node(doc, "div", undefined, "event-row");
        row.append(node(doc, "span", `${index + 1}. ${eventLabel(event)} · ${event.delay_ms} ms`));
        row.append(button(doc, "Edit", () => {
          eventIndex = index; eventDelay = event.delay_ms;
          eventType = Object.keys(event.action)[0];
          if (event.action.Key) { eventUsage = event.action.Key.usage; eventPressed = event.action.Key.pressed; }
          if (event.action.Button) { eventButton = event.action.Button.button; eventPressed = event.action.Button.pressed; }
          if (event.action.Move) { eventDx = event.action.Move.dx; eventDy = event.action.Move.dy; }
          if (event.action.Backend) { eventBackend = event.action.Backend.id; eventPressed = event.action.Backend.pressed; }
          render();
        }, !editor.canEdit));
        row.append(button(doc, "↑", () => void edit("macro", { Move: { from: index, to: index - 1 } }), !editor.canEdit || index === 0));
        row.append(button(doc, "↓", () => void edit("macro", { Move: { from: index, to: index + 1 } }), !editor.canEdit || index === editor.draft.events.length - 1));
        row.append(button(doc, "Remove", () => void edit("macro", { Remove: { at: index } }), !editor.canEdit));
        events.append(row);
      });
      detail.append(events);
      renderMacroForm(detail, editor, caps, countValid);
      const assign = playback;
      assign.append(node(doc, "h3", "Save and assign"), node(doc, "p", `Target: ${view.descriptor.keys.find(item => item.id === key)?.label ?? "key"} · ${view.descriptor.layers.find(item => item.id === layer)?.label ?? "layer"}`, "muted"));
      const bindings = caps.bindings.filter(item => item.slot === state.slot);
      if (!bindings.some(item => item.id === macroBinding)) macroBinding = bindings[0]?.id;
      field(doc, assign, "Playback mode", choice(doc, bindings.map(item => [item.id, item.label]), macroBinding, value => { macroBinding = value; }, !editor.canEdit));
      assign.append(button(doc, "Save and assign to selected key", () => void intent({ type: "saveAndAssignMacro", layer, key, binding: macroBinding }), editor.status !== "Ready" || !countValid || view.busy || !selectedKeyWritable(), "primary"));
      library.append(playback);
      const documents = node(doc, "details", undefined, "card");
      documents.append(node(doc, "summary", "Import or export macro JSON"));
      const file = node(doc, "input"); file.type = "file"; file.accept = "application/json,.json";
      file.addEventListener("change", async () => {
        try { const contents = JSON.parse(await file.files[0].text()); await intent({ type: "importMacroDocument", document: contents }); }
        catch (error) { notice = `Could not read macro file: ${text(error)}`; render(); }
      });
      documents.append(file);
      documents.append(button(doc, "Export macro", async () => {
        const result = await intent({ type: "exportMacroDocument", name: caps.slots.find(item => item.id === state.slot)?.label ?? state.slot, binding: macroBinding });
        if (result.ok) download(`${state.slot}.json`, JSON.stringify(result.outcome.document, null, 2));
      }));
      detail.append(documents);
    }
    layout.append(detail, library);
    panel.append(layout);
  }

  function render() {
    if (disposed) return;
    renderConnection(); renderSidebar(); renderKeyboard();
    status.textContent = notice;
    status.classList.toggle("error", /failed|could not|error|conflict/i.test(notice));
    panel.replaceChildren();
    if (tab === "keymap") renderKeymap();
    else if (tab === "macros") renderMacros();
    else renderLighting();
  }

  function onRecord(event, pressed) {
    if (!view.recording) return;
    const action = recordedKey(event, pressed);
    if (!action) return;
    event.preventDefault();
    void intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  }
  const down = event => { if (!event.repeat) onRecord(event, true); };
  const up = event => onRecord(event, false);
  const blur = () => { if (view.recording) void intent({ type: "recordStop", nowMs: Math.round(now()) }); };
  const visibility = () => { if (doc.hidden) blur(); };
  const unplugged = event => { if (device === event.device) { notice = "Keyboard disconnected."; void disconnect(true); } };
  const unloading = event => {
    if (writes || [view.keymap, view.lighting?.editor, view.picture?.editor, view.macros?.editor].some(item => item?.dirty)) {
      event.preventDefault(); event.returnValue = "";
    }
  };
  doc.addEventListener("keydown", down);
  doc.addEventListener("keyup", up);
  win.addEventListener("blur", blur);
  doc.addEventListener("visibilitychange", visibility);
  win.addEventListener("beforeunload", unloading);
  hid.addEventListener("disconnect", unplugged);
  render();
  return {
    get view() { return view; },
    intent,
    connect,
    disconnect,
    destroy() {
      if (destroying) return destroying;
      disposed = true;
      for (const timer of timers.values()) win.clearTimeout(timer);
      doc.removeEventListener("keydown", down);
      doc.removeEventListener("keyup", up);
      win.removeEventListener("blur", blur);
      doc.removeEventListener("visibilitychange", visibility);
      win.removeEventListener("beforeunload", unloading);
      hid.removeEventListener("disconnect", unplugged);
      root.replaceChildren();
      destroying = (async () => {
        await disconnect(true);
        await Promise.allSettled([...inflight]);
        session.free();
      })();
      return destroying;
    },
  };
}
