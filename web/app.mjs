// SPDX-License-Identifier: GPL-3.0-or-later
import { selectionFilters, selectDevice } from "./transport.mjs";
import { DeviceExecutor } from "./executor.mjs";
import { notificationDevice, NotificationListener } from "./notifications.mjs";
import { HostController } from "./host.mjs";
import { recordedKey, recordedPointer, recordingPolicy } from "./input.mjs";
import { librarySlots } from "./macro_view.mjs";
import { bindingLabel, bindingChanges } from "./keymap_view.mjs";
import { pendingField, numberKind, needsInitialRead } from "./settings_view.mjs";

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

function eventLabel(event) {
  const action = event.action;
  if (action.Key) return `Key ${action.Key.usage} ${action.Key.pressed ? "↓" : "↑"}`;
  if (action.Button) return `Button ${action.Button.button} ${action.Button.pressed ? "↓" : "↑"}`;
  if (action.Move) return `Move ${action.Move.dx}, ${action.Move.dy}`;
  if (action.Backend) return `${action.Backend.id} ${action.Backend.pressed ? "↓" : "↑"}`;
  return "Unknown event";
}

export function mount(root, { codec, hid, storage, now = () => performance.now(), wait, prepareHost } = {}) {
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
  let connectTask = null;
  let eventIndex = null, eventType = "Key", eventUsage = 4, eventButton = 1;
  let eventDx = 0, eventDy = 0, eventDelay = 20, eventPressed = true, eventBackend = "";
  let shortcutKey = view.descriptor.shortcuts?.keys[0]?.usage ?? 4;
  const shortcutMods = new Set();
  let paintColor = "#ffffff", paintReady = false, macroBinding = "counted";
  let recordFixed = false, recordDelay = "50";
  const heldPointerButtons = new Set();
  let macroNames = {};
  let listener = null, notificationStatus = "", generation = null, observationTimer = null;
  let interactionUntil = 0;
  let settingsAutoRead = false;
  let hostController = null, hostMode = null, hostSetting = null, hostUpdateTimer = null;
  let showHostLighting = false;
  let screenSampling = "average", screenX = 500, screenY = 500;
  const macroNameDrafts = new Map();
  const bindingDrafts = new Map();
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
  const archiveDialog = node(doc, "dialog", undefined, "backup-dialog");
  archiveDialog.append(node(doc, "h2", "Native diagnostic archive"),
    node(doc, "p", "Review or copy the exact captured JSON before requesting a download. This view cannot restore an archive.", "muted"));
  const archiveText = node(doc, "textarea");
  archiveText.readOnly = true;
  archiveText.setAttribute("aria-label", "Native archive JSON");
  const archiveActions = node(doc, "div", undefined, "actions");
  archiveDialog.append(archiveText, archiveActions);
  header.append(brand, connection);
  main.append(status, keyboard, panel);
  workspace.append(sidebar, main);
  shell.append(header, workspace, dialog, backupDialog, archiveDialog);
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
    if (response.outcome?.cancelCatalog) executor?.cancelCatalog();
    const discovery = Boolean(view.macros?.discoveryBusy);
    if (previousDiscovery && !discovery) executor?.cancelCatalog();
    previousDiscovery = discovery;
    if (!response.ok) notice = response.error;
    else if (response.outcome?.kind === "hostFailed") {
      notice = `Host lighting restoration failed: ${response.outcome.detail?.message ?? "Read lighting before changing it again."}`;
    }
    else if (response.outcome?.kind === "archiveCaptureFailed") {
      notice = `Diagnostic archive capture failed: ${response.outcome.detail ?? "Read and try again."}`;
    }
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
        archiveCaptured: "Diagnostic archive captured. Export to review its exact JSON.",
      };
      if (saved[response.outcome?.kind]) notice = saved[response.outcome.kind];
    }
    render();
    scheduleObservation();
    maybeReadSettings();
    return response;
  }

  function maybeReadSettings() {
    if (tab !== "settings" || !settingsAutoRead || !needsInitialRead(view.settings?.editor) ||
        !view.settings.editor.canRead || !device || disposed) return;
    settingsAutoRead = false;
    queueMicrotask(() => { if (device && !disposed) void intent({ type: "read", feature: "settings" }); });
  }

  function scheduleObservation() {
    win.clearTimeout(observationTimer);
    observationTimer = null;
    const observation = view.observation;
    if (!device || closing || disposed || !observation?.queued || !observation.canRead ||
        timers.size || view.recording || hostController?.busy) return;
    const active = doc.activeElement;
    if (root.contains(active) && ["INPUT", "SELECT", "TEXTAREA"].includes(active?.tagName)) return;
    const due = Math.max(observation.dueMs ?? 0, interactionUntil);
    observationTimer = win.setTimeout(() => {
      observationTimer = null;
      if (view.observation?.queued && view.observation.canRead && !timers.size && !view.recording && !hostController?.busy &&
          !(root.contains(doc.activeElement) && ["INPUT", "SELECT", "TEXTAREA"].includes(doc.activeElement?.tagName))) {
        void intent({ type: "observeNext", nowMs: Math.round(now()) }, true);
      } else scheduleObservation();
    }, Math.max(0, due - now()));
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
    win.clearTimeout(observationTimer);
    observationTimer = null;
    const attempt = async () => {
      timers.delete(feature);
      const editor = editable(view[feature]);
      if (!editor?.dirty || !device) { scheduleObservation(); return; }
      if (editor.status !== "Ready" || view.connection.kind !== "connected") { scheduleObservation(); return; }
      if (!editor.canApply) {
        // Another command may still be settling; its completion reschedules observation.
        if (view.busy) timers.set(feature, win.setTimeout(attempt, 250));
        else scheduleObservation();
        return;
      }
      await intent({ type: "apply", feature });
      scheduleObservation();
    };
    timers.set(feature, win.setTimeout(attempt, 500));
  }

  async function edit(feature, change, autosave = false) {
    if (hostController?.state.phase === "Preparing") return { ok: false, error: "Wait for host capture to start or stop.", view };
    interactionUntil = now() + 500;
    const result = await intent({ type: "edit", feature, change });
    if (result.ok && autosave) schedule(feature);
    return result;
  }

  function connect() {
    if (device || connecting || closing || disposed) return connectTask ?? Promise.resolve();
    const task = connectSelected();
    connectTask = task;
    void task.then(
      () => { if (connectTask === task) connectTask = null; },
      () => { if (connectTask === task) connectTask = null; },
    );
    return task;
  }

  async function connectSelected() {
    connecting = true;
    render();
    try {
      // Permission must start synchronously inside this click handler.
      const candidates = await hid.requestDevice({ filters: selectionFilters });
      if (disposed) return;
      if (!candidates.length) { notice = "Keyboard selection was cancelled."; return; }
      device = selectDevice(candidates);
      executor = new DeviceExecutor(device, codec.BrowserOperation, storage, wait);
      const result = handle(JSON.parse(session.dispatch('{"type":"connect"}')));
      if (!result.ok) throw new Error(result.error);
      generation = result.outcome.generation;
      hostController = new HostController({
        codec, dispatch: input => intent(input), executor,
        changed: state => {
          if (state.problem) notice = `Host lighting: ${state.problem}`;
          else if (state.phase === "Active") notice = `Host lighting active from ${state.source}. Stop to restore the original onboard lighting.`;
          else if (state.phase === "Idle" && view.host?.phase === "Idle") notice = "Host lighting stopped. Original onboard lighting restored.";
          render();
          scheduleObservation();
        },
        prepare: prepareHost, environment: win,
      });
      const selectedDevice = device;
      const selectedGeneration = generation;
      try {
        const notifications = notificationDevice(candidates, device);
        listener = new NotificationListener(notifications, (reportId, payload) => {
          if (disposed || device !== selectedDevice || generation !== selectedGeneration) return;
          handle(JSON.parse(session.dispatch(JSON.stringify({
            type: "notification", generation: selectedGeneration, reportId, payload: [...payload], nowMs: Math.round(now()),
          }))));
        });
        await listener.open();
        if (disposed || device !== selectedDevice) return;
        notificationStatus = "Live onboard updates available";
      } catch (error) {
        notificationStatus = `${text(error)} Manual Read and reconnect remain available.`;
        await listener?.close().catch(() => {});
        listener = null;
      }
      if (disposed || device !== selectedDevice) return;
      const selected = device;
      void storage.macroNames?.(selected)?.then(names => {
        if (device !== selected) return;
        macroNames = names;
        render();
      }).catch(error => { notice = `Local macro names could not be read: ${text(error)}`; render(); });
      notice = `Reading ${device.productName || "Nia87"}…`;
      render();
      await intent({ type: "read", feature: "keymap" });
      if (disposed || device !== selectedDevice) return;
      await intent({ type: "read", feature: "lighting" });
      if (disposed || device !== selectedDevice) return;
      notice = view.keymap.status === "Ready" && view.lighting?.editor.status === "Ready"
        ? "Keyboard ready. Select a key or feature to edit."
        : "Some keyboard data could not be read. Use the feature Read control to retry.";
      render();
      if (!disposed) void intent({ type: "discoverMacros" }, true);
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
    render();
    win.clearTimeout(hostUpdateTimer);
    hostUpdateTimer = null;
    for (const timer of timers.values()) win.clearTimeout(timer);
    timers.clear();
    let stopError = null;
    if (hostController?.busy) {
      try { await hostController.stop(); }
      catch (error) { stopError = text(error); }
    }
    if (!force && (stopError || view.host?.phase !== "Idle" ||
        (hostController?.state.problem && view.lighting?.editor.status !== "Ready"))) {
      notice = `Host lighting restoration could not be verified${stopError ? `: ${stopError}` : ""}. Stay connected and Read lighting before disconnecting.`;
      closing = false;
      render();
      return;
    }
    hostController = null;
    win.clearTimeout(observationTimer);
    observationTimer = null;
    const oldListener = listener;
    listener = null;
    generation = null;
    notificationStatus = "";
    settingsAutoRead = false;
    handle(JSON.parse(session.dispatch('{"type":"disconnect"}')));
    const old = executor;
    executor = null;
    device = null;
    heldPointerButtons.clear();
    macroNames = {};
    macroNameDrafts.clear();
    bindingDrafts.clear();
    if (!force) notice = "Disconnected. Choose a keyboard to reconnect.";
    try { await oldListener?.close(); await old?.close(); }
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

  function nameFor(slot) {
    return macroNameDrafts.get(slot) ?? macroNames[slot] ?? "";
  }

  async function saveMacroName(slot) {
    if (!device) return;
    const name = nameFor(slot);
    try {
      if (typeof storage.saveMacroName !== "function") throw new Error("Local name storage is unavailable.");
      await storage.saveMacroName(device, slot, name);
      if (name.trim()) macroNames[slot] = name;
      else delete macroNames[slot];
      macroNameDrafts.delete(slot);
      notice = "Local macro name saved. Keyboard configuration is unchanged.";
    } catch (error) { notice = `Local macro name was not saved: ${text(error)}`; }
    render();
  }

  async function stopRecording() {
    heldPointerButtons.clear();
    if (view.recording) await intent({ type: "recordStop", nowMs: Math.round(now()) });
  }

  async function startRecording(capabilities) {
    try {
      const policy = recordingPolicy({ fixed: recordFixed, delay: recordDelay }, capabilities);
      if (view.macros?.discoveryBusy) {
        const selected = device;
        const cancelled = await intent({ type: "cancelCatalog" });
        if (!cancelled.ok) return;
        // The active report settles before recording starts; queued slots are cancelled.
        await Promise.allSettled([...inflight]);
        if (disposed || device !== selected || view.connection.kind !== "connected") return;
      }
      const result = await intent({ type: "recordStart", policy });
      if (result.ok) doc.getElementById("record-capture")?.focus();
    } catch (error) { notice = text(error); render(); }
  }

  function selectedHostMode() {
    const modes = view.lighting?.capabilities?.host_modes ?? [];
    return modes.find(mode => mode.id === hostMode) ?? modes[0];
  }

  function currentHostSetting(mode) {
    if (!mode?.parameters) return null;
    if (hostSetting?.effect !== mode.id) hostSetting = JSON.parse(JSON.stringify(mode.parameters.default));
    return hostSetting;
  }

  function queueHostUpdate() {
    win.clearTimeout(hostUpdateTimer);
    if (hostController?.state.phase !== "Active") return;
    hostUpdateTimer = win.setTimeout(() => {
      hostUpdateTimer = null;
      const setting = JSON.parse(JSON.stringify(hostSetting));
      void hostController?.update(setting).catch(error => {
        notice = `Host lighting parameters: ${text(error)}`;
        render();
      });
    }, 120);
  }

  function changeHostSetting(change) {
    hostSetting = { ...hostSetting, ...change };
    queueHostUpdate();
    render();
  }

  async function startHost() {
    const mode = selectedHostMode();
    if (!hostController || !mode) return;
    const setting = currentHostSetting(mode);
    const options = mode.source === "ScreenAverage"
      ? { sampling: screenSampling, x: screenX, y: screenY } : {};
    try { await hostController.start(mode, setting, options); }
    catch (error) { notice = `Host lighting: ${text(error)}`; render(); }
  }

  async function stopHost() {
    win.clearTimeout(hostUpdateTimer);
    hostUpdateTimer = null;
    try { await hostController?.stop(); }
    catch (error) { notice = `Host lighting restoration failed: ${text(error)}`; render(); }
  }

  function renderConnection() {
    connection.replaceChildren();
    connection.append(node(doc, "span", device ? `${device.productName || "Nia87"} · ${device.vendorId.toString(16)}:${device.productId.toString(16)}` : "No keyboard selected", "device-name"));
    if (device) connection.append(node(doc, "span", notificationStatus, listener ? "live-status" : "live-status unavailable"));
    connection.append(button(doc, device ? "Connected" : "Choose keyboard", connect, Boolean(device || connecting || closing), "primary"));
    if (hostController?.busy) {
      const stop = button(doc, hostController.state.phase === "Stopping" ? "Restoring…" : "Stop host lighting", () => void stopHost(), closing || hostController.state.phase === "Stopping", "host-stop");
      stop.dataset.hostStop = "true";
      connection.append(stop);
    }
    if (device) connection.append(button(doc, "Disconnect", () => void disconnect(), Boolean(writes || closing || connecting)));
  }

  function renderSidebar() {
    sidebar.replaceChildren(node(doc, "p", "WORKSPACE", "eyebrow"));
    for (const [id, label] of [["keymap", "Keymap"], ["macros", "Macros"], ["lighting", "Lighting"], ["settings", "Settings"], ["diagnostics", "Diagnostics"]]) {
      const dirty = editable(view[id])?.dirty;
      const item = button(doc, `${label}${dirty ? " •" : ""}`, () => {
        tab = id; render();
        if (id === "settings") { settingsAutoRead = true; maybeReadSettings(); }
      }, false, tab === id ? "nav-item active" : "nav-item");
      item.setAttribute("aria-current", tab === id ? "page" : "false");
      sidebar.append(item);
    }
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
    const lighting = view.lighting;
    const pictureMode = !showHostLighting && lighting?.editor.draft?.effect === view.picture?.capabilities?.lighting_effect;
    const modeLabel = showHostLighting ? selectedHostMode()?.label : lighting?.capabilities.effects.find(effect => effect.id === lighting.editor.draft?.effect)?.label;
    heading.append(node(doc, "div", `${view.descriptor.device_name} · ${tab === "lighting" ? modeLabel ?? "Lighting" : tab === "macros" ? "Assignment target" : tab === "keymap" ? "Key bindings" : "Keyboard overview"}`));
    if (tab === "lighting" && lighting) {
      const caps = lighting.capabilities;
      const choices = [
        ...caps.effects.map(effect => [`onboard:${effect.id}`, effect.label]),
        ...caps.host_modes.map(mode => [`host:${mode.id}`, mode.label]),
      ];
      const selected = showHostLighting ? `host:${selectedHostMode()?.id}` : lighting.editor.draft ? `onboard:${lighting.editor.draft.effect}` : "";
      field(doc, heading, "Lighting mode", choice(doc, choices, selected, async value => {
        if (value.startsWith("host:")) {
          showHostLighting = true; hostMode = value.slice(5); hostSetting = null; paintReady = false;
          render();
        } else {
          const result = await edit("lighting", { Effect: value.slice(8) }, true);
          if (result?.ok) { showHostLighting = false; paintReady = false; render(); }
        }
      }, !lighting.editor.canSelectEffect || Boolean(hostController?.busy)));
    }
    if (tab === "keymap" || tab === "macros") {
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
      const mapped = (tab === "keymap" || tab === "macros") && view.keymap.draft?.[layer]?.[item.id]
        ? bindingLabel(view.descriptor, view.keymap.draft[layer][item.id], layer, item.id, macroNames) : null;
      const changed = mapped && !same(view.keymap.baseline?.bindings?.[layer]?.[item.id], view.keymap.draft[layer][item.id]);
      const element = button(doc, mapped?.compact ?? item.label, () => {
        key = item.id;
        if (tab === "lighting" && pictureMode && paintReady && view.picture?.editor?.canEdit && view.picture.capabilities.keys.includes(key)) {
          void edit("picture", { Color: { key, color: rgb(paintColor) } }, true);
        } else render();
      }, false, `key ${key === item.id ? "selected" : ""} ${changed ? "changed" : ""} ${view.descriptor.layers.find(entry => entry.id === layer)?.read_only_keys.includes(item.id) || !item.writable ? "protected" : ""}`);
      element.title = mapped?.full ?? item.label;
      element.style.left = `${item.x * 43 + 7}px`;
      element.style.top = `${item.y * 43 + 7}px`;
      element.style.width = `${item.width * 43 - 4}px`;
      element.style.height = `${item.height * 43 - 4}px`;
      if (tab === "lighting" && pictureMode && colors[item.id]) element.style.setProperty("--key-color", hex(colors[item.id]));
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
    card.append(node(doc, "p", protectedKey ? "Reserved for an onboard command" : `Current: ${bindingLabel(view.descriptor, editor.draft?.[layer]?.[key], layer, key, macroNames).full}`, "muted"));
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
    const changes = bindingChanges(view.descriptor, editor.baseline, editor.draft, macroNames);
    if (changes.length) {
      const review = node(doc, "div", undefined, "card keymap-changes");
      review.append(node(doc, "h3", "Changes to save"));
      for (const change of changes) review.append(node(doc, "p", `${change.layer} / ${change.key}: ${change.before} → ${change.after}`));
      panel.append(review);
    }
  }

  function renderLighting() {
    const state = view.lighting;
    panel.append(node(doc, "h2", "Lighting"), node(doc, "p", "Onboard effects, brightness and per-key RGB.", "muted"));
    if (!state) return;
    const editor = state.editor, draft = editor.draft, caps = state.capabilities;
    panel.append(actions(editor, "lighting", true));
    if (showHostLighting) { renderHost(); return; }
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
    if (effect?.options.length) field(doc, card, draft.effect === view.picture?.capabilities?.lighting_effect ? "Layer" : "Style / direction", choice(doc, effect.options.map(item => [item.id, item.label]), draft.option, value => void edit("lighting", { Option: value }, true), !editor.canEdit));
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

  function renderHost() {
    const modes = view.lighting?.capabilities?.host_modes ?? [];
    if (!modes.length) return;
    const mode = selectedHostMode();
    const host = hostController?.state ?? { phase: "Idle", source: "", problem: null };
    const editor = view.lighting.editor;
    const settings = view.settings?.editor;
    const required = mode.requires_enabled_setting;
    const enabled = !required || (settings?.status === "Ready" && !settings.dirty &&
      settings.baseline?.content?.Editable?.[required]?.Toggle === true);
    const clean = editor.status === "Ready" && Boolean(editor.draft) && !editor.dirty;
    const canStart = Boolean(device && hostController && host.phase === "Idle" && view.host?.phase === "Idle" &&
      !view.busy && !view.recording && !timers.size && clean && enabled);
    const card = node(doc, "div", undefined, "card host-card");
    card.append(node(doc, "h3", "Host lighting"));
    card.append(node(doc, "p", "Capture stays local to this browser. Samples are sent only to the selected keyboard. Stop to restore its original onboard lighting.", "muted"));
    card.append(node(doc, "h3", mode.label));
    if (mode.source === "ScreenAverage") {
      field(doc, card, "Screen sample", choice(doc, [["average", "Average color"], ["point", "Selected point"]], screenSampling,
        value => { screenSampling = value; render(); }, host.phase !== "Idle"));
      if (screenSampling === "point") {
        const point = node(doc, "div", undefined, "host-point");
        for (const [label, coordinate, update] of [["X", screenX, value => { screenX = value; }], ["Y", screenY, value => { screenY = value; }]]) {
          const input = number(doc, coordinate, 0, 1000, value => {
            if (Number.isInteger(value) && value >= 0 && value <= 1000) update(value);
            else { notice = "Choose a screen point from 0 to 1000."; render(); }
          }, host.phase !== "Idle");
          field(doc, point, `${label} · 0–1000`, input);
        }
        card.append(point);
      }
      card.append(node(doc, "p", "Choose a tab, window or screen in the browser share dialog.", "muted"));
    } else card.append(node(doc, "p", "Choose a tab or screen with Share audio enabled. No microphone fallback is used.", "muted"));
    if (mode.parameters) {
      const value = currentHostSetting(mode), schema = mode.parameters.schema;
      const params = node(doc, "div", undefined, "host-parameters");
      if (schema.brightness) {
        const [min, max] = range(schema.brightness);
        const slider = node(doc, "input");
        slider.type = "range"; slider.min = String(min); slider.max = String(max); slider.value = String(value.brightness);
        slider.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping";
        slider.addEventListener("change", () => changeHostSetting({ brightness: Number(slider.value) }));
        field(doc, params, `Brightness · ${value.brightness}`, slider);
      }
      if (schema.options.length) field(doc, params, "Style", choice(doc, schema.options.map(item => [item.id, item.label]),
        value.option, option => changeHostSetting({ option }), host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping"));
      if (schema.color) {
        if (schema.color !== "Rainbow") {
          const picker = node(doc, "input"); picker.type = "color";
          picker.value = value.color?.Rgb ? hex(value.color.Rgb) : "#ffffff";
          picker.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping" || value.color === "Rainbow";
          picker.addEventListener("change", () => changeHostSetting({ color: { Rgb: rgb(picker.value) } }));
          field(doc, params, "Color", picker);
        }
        if (schema.color === "FixedOrRainbow" || schema.color === "Rainbow") {
          const label = node(doc, "label", "Rainbow", "host-rainbow");
          const toggle = node(doc, "input"); toggle.type = "checkbox"; toggle.checked = value.color === "Rainbow";
          toggle.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping";
          toggle.addEventListener("change", () => changeHostSetting({ color: toggle.checked ? "Rainbow" : { Rgb: [255, 255, 255] } }));
          label.prepend(toggle); params.append(label);
        }
      }
      card.append(params);
    }
    if (!enabled) {
      card.append(node(doc, "p", "Read Settings, enable and save Backlight, then return here.", "muted"));
      card.append(button(doc, "Open Settings", () => { tab = "settings"; settingsAutoRead = true; render(); maybeReadSettings(); }));
    } else if (!clean && host.phase === "Idle") card.append(node(doc, "p", "Read lighting and save or revert its draft before starting capture.", "muted"));
    const controls = node(doc, "div", undefined, "actions");
    controls.append(button(doc, host.phase === "Preparing" ? "Choose capture…" : "Start host lighting", () => void startHost(), !canStart, "primary"));
    if (host.phase !== "Idle") {
      const stop = button(doc, host.phase === "Stopping" ? "Restoring…" : "Stop and restore", () => void stopHost(), host.phase === "Stopping", "host-stop");
      stop.dataset.hostStop = "true";
      controls.append(stop);
    }
    card.append(controls);
    if (host.phase !== "Idle") card.append(node(doc, "p", `${host.phase}${host.source ? ` · ${host.source}` : ""}`, "host-phase"));
    if (host.problem) card.append(node(doc, "p", host.problem, "error"));
    panel.append(card);
  }

  function renderSettings() {
    panel.append(node(doc, "h2", "Settings"), node(doc, "p", "Onboard timing and behavior. Each setting saves independently after 500 ms.", "muted"));
    const state = view.settings;
    if (!state) return;
    const editor = state.editor;
    panel.append(actions(editor, "settings", true));
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
        toggle.addEventListener("change", () => void edit("settings", { id: setting.id, value: { Toggle: toggle.checked } }, true));
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
            choiceValue => void edit("settings", { id: setting.id, value: { Number: choiceValue === "disabled" ? 0 : limits.min } }, true), locked);
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
        slider.addEventListener("change", () => void edit("settings", { id: setting.id, value: { Number: Number(slider.value) } }, true));
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
            notice = `Choose ${setting.label} from ${limits.min} to ${limits.max} ${limits.unit}.`;
            render();
            return;
          }
          void edit("settings", { id: setting.id, value: { Number: next } }, true);
        });
        controls.append(slider, numberInput, node(doc, "span", limits.unit, "muted"));
        card.append(controls);
      }
      list.append(card);
    }
    panel.append(list);
  }

  function renderDiagnostics() {
    panel.append(node(doc, "h2", "Diagnostics"),
      node(doc, "p", "Capture the keyboard's native configuration for inspection or export. This is read-only; archives cannot be imported or restored here.", "muted"));
    const archive = view.archive;
    if (!archive) { panel.append(node(doc, "p", "This keyboard does not advertise diagnostic archives.", "muted")); return; }
    const card = node(doc, "div", undefined, "card");
    card.append(node(doc, "h3", "Native archive"));
    card.append(node(doc, "p", `Format: ${archive.capabilities.format_id} · Limit: ${archive.capabilities.max_bytes} bytes`, "muted"));
    if (archive.status.kind === "ready") card.append(node(doc, "p", `${archive.capturedBytes} bytes captured.`, "muted"));
    else if (archive.status.kind === "failed") {
      card.append(node(doc, "p", `Latest capture failed: ${archive.status.problem}`, "error"));
      if (archive.capturedBytes) card.append(node(doc, "p", `An earlier ${archive.capturedBytes}-byte capture remains available for export.`, "muted"));
    } else card.append(node(doc, "p", "No archive captured in this session.", "muted"));
    const controls = node(doc, "div", undefined, "actions");
    controls.append(button(doc, "Capture archive", () => void intent({ type: "captureArchive" }), !archive.canCapture, "primary"));
    controls.append(button(doc, "Review / export JSON", async () => {
      const result = await intent({ type: "exportArchive" });
      if (!result.ok || result.outcome?.kind !== "archiveExported") return;
      const contents = result.outcome.document;
      archiveText.value = contents;
      archiveActions.replaceChildren(
        button(doc, "Download JSON", () => {
          download("byakko-native-archive.json", contents);
          notice = "Archive download requested. The captured JSON remains visible for copying.";
          render();
        }, false, "primary"),
        button(doc, "Close", () => {
          if (typeof archiveDialog.close === "function") archiveDialog.close();
          else archiveDialog.removeAttribute("open");
        }),
      );
      if (typeof archiveDialog.showModal === "function") archiveDialog.showModal();
      else archiveDialog.setAttribute("open", "");
      notice = "Native archive JSON prepared for review. Download must be requested separately.";
      render();
    }, !archive.canExport));
    card.append(controls);
    panel.append(card);
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
      library.append(button(doc, "Retry discovery", () => void intent({ type: "discoverMacros" }), state.discoveryBusy));
    } else if (!state.discoveryBusy) {
      const used = Object.values(state.occupancy).filter(value => value === "Configured" || value === "Opaque").length;
      library.append(node(doc, "p", `${used} of ${caps.slots.length} slots used`, "muted"));
    }
    const detail = node(doc, "div", undefined, "macro-detail");
    detail.append(node(doc, "h3", `${nameFor(state.slot).trim() || caps.slots.find(item => item.id === state.slot)?.label || state.slot} · Editor`));
    detail.append(actions(editor, "macro"));
    if (editor.baseline) {
      const nameRow = node(doc, "div", undefined, "name-row");
      const nameInput = node(doc, "input");
      nameInput.type = "text";
      nameInput.placeholder = "Local macro name";
      nameInput.value = nameFor(state.slot);
      nameInput.disabled = view.recording;
      nameInput.addEventListener("input", () => { macroNameDrafts.set(state.slot, nameInput.value); });
      nameRow.append(nameInput, button(doc, "Save name", () => void saveMacroName(state.slot), !device || view.recording));
      detail.append(nameRow);
    }
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
      const fixedLabel = node(doc, "label", "Fixed delay");
      const fixedInput = node(doc, "input");
      fixedInput.type = "checkbox"; fixedInput.checked = recordFixed; fixedInput.disabled = view.recording;
      fixedInput.addEventListener("change", () => { recordFixed = fixedInput.checked; render(); });
      fixedLabel.prepend(fixedInput);
      recordRow.append(fixedLabel);
      if (recordFixed) {
        const delayInput = node(doc, "input");
        delayInput.type = "number"; delayInput.min = String(Math.max(1, caps.delays_ms.start));
        delayInput.max = String(caps.delays_ms.end); delayInput.value = recordDelay;
        delayInput.disabled = view.recording;
        delayInput.setAttribute("aria-label", "Fixed recording delay in milliseconds");
        delayInput.addEventListener("input", () => { recordDelay = delayInput.value; });
        recordRow.append(delayInput, node(doc, "span", "ms", "muted"));
      } else recordRow.append(node(doc, "span", `Measured timing · ${Math.min(50, caps.delays_ms.end)} ms final wait`, "muted"));
      recordRow.append(button(doc, view.recording ? "Stop recording" : "Record input", async () => {
        if (view.recording) await stopRecording();
        else await startRecording(caps);
      }, !view.recording && (!editor.canEdit || !countValid)));
      const capture = node(doc, "div", view.recording ? "Recording keys and pointer buttons. Click here for pointer actions; Stop releases held inputs." : "Recording is local until you save.", "record-capture");
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
      const assigned = view.keymap.draft?.[layer]?.[key];
      const currentBinding = bindings.find(item => same(item.action, assigned))?.id;
      macroBinding = bindingDrafts.get(state.slot) ?? currentBinding ?? bindings[0]?.id;
      field(doc, assign, "Playback mode", choice(doc, bindings.map(item => [item.id, item.label]), macroBinding, value => { macroBinding = value; bindingDrafts.set(state.slot, value); }, !editor.canEdit));
      assign.append(button(doc, "Save and assign to selected key", () => void intent({ type: "saveAndAssignMacro", layer, key, binding: macroBinding }), editor.status !== "Ready" || !countValid || view.busy || !selectedKeyWritable(), "primary"));
      library.append(playback);
      const documents = node(doc, "details", undefined, "card");
      documents.append(node(doc, "summary", "Import or export macro JSON"));
      const file = node(doc, "input"); file.type = "file"; file.accept = "application/json,.json";
      file.disabled = view.recording;
      file.addEventListener("change", async () => {
        try {
          if (!file.files?.[0]) return;
          if (file.files[0].size > 64 * 1024) throw new Error("Macro JSON exceeds the 64 KiB import limit.");
          const contents = JSON.parse(await file.files[0].text());
          const result = await intent({ type: "importMacroDocument", document: contents });
          if (result.ok) {
            macroNameDrafts.set(state.slot, result.outcome.name);
            if (contents.backend_id === view.descriptor.backend_id && caps.bindings.some(binding => binding.slot === state.slot && binding.id === result.outcome.binding)) {
              macroBinding = result.outcome.binding;
              bindingDrafts.set(state.slot, macroBinding);
            } else {
              bindingDrafts.delete(state.slot);
            }
            render();
          }
        }
        catch (error) { notice = `Could not read macro file: ${text(error)}`; render(); }
      });
      documents.append(file);
      documents.append(button(doc, "Export macro", async () => {
        const result = await intent({ type: "exportMacroDocument", name: nameFor(state.slot), binding: bindingDrafts.get(state.slot) ?? null });
        if (result.ok) download(`${state.slot}.json`, JSON.stringify(result.outcome.document, null, 2));
      }, view.recording));
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
    else if (tab === "lighting") renderLighting();
    else if (tab === "settings") renderSettings();
    else renderDiagnostics();
    if (hostController?.state.phase === "Preparing") {
      for (const control of [...panel.querySelectorAll("button,input,select"), ...keyboard.querySelectorAll("button")]) {
        if (!control.dataset.hostStop) control.disabled = true;
      }
    }
  }

  function onRecord(event, pressed) {
    if (!view.recording) return;
    if ((event.code === "Enter" || event.code === "Space") && event.target?.closest?.("button")) return;
    const action = recordedKey(event, pressed);
    if (!action) return;
    event.preventDefault();
    void intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  }
  const down = event => { if (!event.repeat) onRecord(event, true); };
  const up = event => onRecord(event, false);
  const pointerDown = event => {
    if (!view.recording || !event.target?.closest?.(".record-capture")) return;
    const action = recordedPointer(event, true, view.macros.capabilities);
    if (!action) return;
    event.preventDefault();
    heldPointerButtons.add(event.button);
    void intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  };
  const pointerUp = event => {
    if (!view.recording || !heldPointerButtons.delete(event.button)) return;
    const action = recordedPointer(event, false, view.macros.capabilities);
    if (action) void intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  };
  const pointerCancel = () => {
    if (!view.recording) { heldPointerButtons.clear(); return; }
    for (const button of heldPointerButtons) {
      const action = recordedPointer({ button }, false, view.macros.capabilities);
      if (action) void intent({ type: "recordInput", action, nowMs: Math.round(now()) });
    }
    heldPointerButtons.clear();
  };
  const contextMenu = event => {
    if (view.recording && event.target?.closest?.(".record-capture")) event.preventDefault();
  };
  const blur = () => { if (view.recording) void stopRecording(); };
  const visibility = () => { if (doc.hidden) blur(); };
  const unplugged = event => {
    if (device === event.device) {
      notice = "Keyboard disconnected.";
      void disconnect(true);
    } else if (listener?.device === event.device) {
      const old = listener;
      listener = null;
      notificationStatus = "Live onboard updates unavailable. Use Read or reconnect.";
      void old.close().catch(() => {});
      render();
    }
  };
  const focusOut = () => { win.setTimeout(scheduleObservation, 0); };
  const unloading = event => {
    if (writes || hostController?.busy || view.host?.phase !== "Idle" ||
        [view.keymap, view.lighting?.editor, view.picture?.editor, view.macros?.editor, view.settings?.editor].some(item => item?.dirty)) {
      event.preventDefault(); event.returnValue = "";
    }
  };
  doc.addEventListener("keydown", down);
  doc.addEventListener("keyup", up);
  doc.addEventListener("pointerdown", pointerDown);
  doc.addEventListener("pointerup", pointerUp);
  doc.addEventListener("pointercancel", pointerCancel);
  doc.addEventListener("contextmenu", contextMenu);
  root.addEventListener("focusout", focusOut);
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
      win.clearTimeout(observationTimer);
      doc.removeEventListener("keydown", down);
      doc.removeEventListener("keyup", up);
      doc.removeEventListener("pointerdown", pointerDown);
      doc.removeEventListener("pointerup", pointerUp);
      doc.removeEventListener("pointercancel", pointerCancel);
      doc.removeEventListener("contextmenu", contextMenu);
      root.removeEventListener("focusout", focusOut);
      win.removeEventListener("blur", blur);
      doc.removeEventListener("visibilitychange", visibility);
      win.removeEventListener("beforeunload", unloading);
      hid.removeEventListener("disconnect", unplugged);
      root.replaceChildren();
      destroying = (async () => {
        if (connectTask) await Promise.allSettled([connectTask]);
        await disconnect(true);
        await Promise.allSettled([...inflight]);
        session.free();
      })();
      return destroying;
    },
  };
}
