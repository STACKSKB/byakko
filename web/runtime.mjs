// SPDX-License-Identifier: GPL-3.0-or-later
import { selectionFilters, selectDevice } from "./transport.mjs";
import { DeviceExecutor } from "./executor.mjs";
import { notificationDevice, NotificationListener } from "./notifications.mjs";
import { HostController } from "./host.mjs";
import { editable, text } from "./widgets.mjs";

// Owns one connection and its ordered effects. Editable feature values stay in core.
export function createRuntime({ codec, hid, storage, now, wait, prepareHost, win,
  changed, isEditing, connected, disconnected }) {
  const session = new codec.BrowserSession();
  let view = JSON.parse(session.view());
  let device = null, executor = null, connecting = false, closing = false;
  let notice = "Choose your Nia87 to load its keymap and lighting.", noticeError = false;
  let writes = 0, disposed = false, previousDiscovery = false, destroying = null, connectTask = null;
  let listener = null, notificationStatus = "", generation = null, observationTimer = null;
  let interactionUntil = 0, hostController = null;
  const timers = new Map(), inflight = new Set();

  function handle(response, background = false) {
    view = response.view;
    executor?.setRecording(view.recording);
    if (response.outcome?.cancelCatalog) executor?.cancelCatalog();
    const discovery = Boolean(view.macros?.discoveryBusy);
    if (previousDiscovery && !discovery) executor?.cancelCatalog();
    previousDiscovery = discovery;
    if (!response.ok) { notice = response.error; noticeError = true; }
    else if (response.outcome?.kind === "hostFailed") {
      noticeError = true;
      notice = `Host lighting restoration failed: ${response.outcome.detail?.message ?? "Read lighting before changing it again."}`;
    }
    else if (response.outcome?.kind === "archiveCaptureFailed") {
      noticeError = true;
      notice = `Diagnostic archive capture failed: ${response.outcome.detail ?? "Read and try again."}`;
    }
    else if (response.outcome?.kind === "failed" || ["picturePreparationFailed", "catalogFailed", "assignmentFailed"].includes(response.outcome?.kind)) {
      noticeError = true;
      notice = JSON.stringify(response.outcome.detail ?? response.outcome.kind);
    } else if (response.outcome?.kind === "conflict") { noticeError = true; notice = "The keyboard changed. Review and read this feature again."; }
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
      if (saved[response.outcome?.kind]) { noticeError = false; notice = saved[response.outcome.kind]; }
    }
    changed(background);
    scheduleObservation();
    return response;
  }

  function scheduleObservation() {
    win.clearTimeout(observationTimer);
    observationTimer = null;
    const observation = view.observation;
    if (!device || closing || disposed || !observation?.queued || !observation.canRead ||
        timers.size || view.recording || hostController?.busy) return;
    if (isEditing()) return;
    const due = Math.max(observation.dueMs ?? 0, interactionUntil);
    observationTimer = win.setTimeout(() => {
      observationTimer = null;
      if (view.observation?.queued && view.observation.canRead && !timers.size && !view.recording && !hostController?.busy &&
          !isEditing()) {
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
      changed(true);
      try {
        const completion = await executor.run(command);
        if (disposed) { command = null; continue; }
        const result = handle(JSON.parse(session.accept(JSON.stringify(completion))), true);
        last = result;
        command = result.command;
      } finally {
        if (write) writes--;
        changed(true);
      }
    }
    return last;
  }

  async function intent(input, background = false) {
    const result = handle(JSON.parse(session.dispatch(JSON.stringify(input))), background);
    if (result.ok && !result.command) {
      const messages = {
        edit: "Draft updated. Save to keyboard when ready.",
        revert: "Draft reverted.",
        initializeMacro: "Empty macro initialized. Add events, then save.",
        importMacroDocument: "Macro document staged. Save to keyboard when ready.",
        recordStart: "Recording locally. Press keys, then stop to release held inputs.",
        recordStop: "Recording stopped. Review events before saving.",
      };
      if (messages[input.type]) { noticeError = false; notice = messages[input.type]; changed(); }
    }
    if (result.command) {
      const running = execute(result.command).catch(error => {
        noticeError = true;
        notice = text(error);
        changed(true);
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
    changed();
    try {
      // Permission must start synchronously inside this click handler.
      const candidates = await hid.requestDevice({ filters: selectionFilters });
      if (disposed) return;
      if (!candidates.length) { noticeError = false; notice = "Keyboard selection was cancelled."; return; }
      device = selectDevice(candidates);
      executor = new DeviceExecutor(device, codec.BrowserOperation, storage, wait);
      const result = handle(JSON.parse(session.dispatch('{"type":"connect"}')));
      if (!result.ok) throw new Error(result.error);
      generation = result.outcome.generation;
      hostController = new HostController({
        codec, dispatch: input => intent(input), executor,
        changed: state => {
          noticeError = Boolean(state.problem);
          if (state.problem) notice = `Host lighting: ${state.problem}`;
          else if (state.phase === "Active") notice = `Host lighting active from ${state.source}. Stop to restore the original onboard lighting.`;
          else if (state.phase === "Idle" && view.host?.phase === "Idle") notice = "Host lighting stopped. Original onboard lighting restored.";
          changed(true);
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
          }))), true);
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
      connected(device, generation);
      noticeError = false;
      notice = `Reading ${device.productName || "Nia87"}…`;
      changed();
      await intent({ type: "read", feature: "keymap" });
      if (disposed || device !== selectedDevice) return;
      await intent({ type: "read", feature: "lighting" });
      if (disposed || device !== selectedDevice) return;
      noticeError = false;
      notice = view.keymap.status === "Ready" && view.lighting?.editor.status === "Ready"
        ? "Keyboard ready. Select a key or feature to edit."
        : "Some keyboard data could not be read. Use the feature Read control to retry.";
      changed();
      if (!disposed) void intent({ type: "discoverMacros" }, true);
    } catch (error) {
      noticeError = true;
      notice = `Could not open the keyboard: ${text(error)}`;
      if (device) await disconnect(true);
    } finally {
      connecting = false;
      changed();
    }
  }

  async function disconnect(force = false) {
    if (!device || closing || (writes && !force)) return;
    closing = true;
    changed();
    for (const timer of timers.values()) win.clearTimeout(timer);
    timers.clear();
    let stopError = null;
    if (hostController?.busy) {
      try { await hostController.stop(); }
      catch (error) { stopError = text(error); }
    }
    if (!force && (stopError || view.host?.phase !== "Idle" ||
        (hostController?.state.problem && view.lighting?.editor.status !== "Ready"))) {
      noticeError = true;
      notice = `Host lighting restoration could not be verified${stopError ? `: ${stopError}` : ""}. Stay connected and Read lighting before disconnecting.`;
      closing = false;
      changed();
      return;
    }
    hostController = null;
    win.clearTimeout(observationTimer);
    observationTimer = null;
    const oldListener = listener;
    listener = null;
    generation = null;
    notificationStatus = "";
    handle(JSON.parse(session.dispatch('{"type":"disconnect"}')));
    const old = executor;
    executor = null;
    device = null;
    disconnected();
    if (!force) { noticeError = false; notice = "Disconnected. Choose a keyboard to reconnect."; }
    try { await oldListener?.close(); await old?.close(); }
    catch (error) { noticeError = true; notice = `Device access ended: ${text(error)}`; }
    finally { closing = false; changed(); }
  }

  function announce(message, error = false, background = false) {
    notice = message; noticeError = error; changed(background);
  }
  function identity() { return { device, generation }; }
  function isCurrent(target) {
    return !disposed && Boolean(device) && device === target.device && generation === target.generation;
  }
  async function startRecording(policy, isWanted = () => true) {
    if (view.macros?.discoveryBusy) {
      const target = identity();
      const cancelled = await intent({ type: "cancelCatalog" });
      if (!cancelled.ok) return cancelled;
      await Promise.allSettled([...inflight]);
      if (!isCurrent(target)) return { ok: false };
    }
    if (!isWanted()) return { ok: false };
    return intent({ type: "recordStart", policy });
  }
  async function stopHost() {
    try { await hostController?.stop(); }
    catch (error) { announce(`Host lighting restoration failed: ${text(error)}`, true); }
  }
  const unplugged = event => {
    if (device === event.device) {
      announce("Keyboard disconnected."); void disconnect(true);
    } else if (listener?.device === event.device) {
      const old = listener; listener = null;
      notificationStatus = "Live onboard updates unavailable. Use Read or reconnect.";
      void old.close().catch(() => {}); changed();
    }
  };
  hid.addEventListener("disconnect", unplugged);
  return {
    get view() { return view; },
    get connection() { return { device, connecting, closing, writes, listener: Boolean(listener), notificationStatus }; },
    get notice() { return { message: notice, error: noticeError }; },
    get host() { return hostController?.state ?? { phase: "Idle", source: "", problem: null }; },
    get hostBusy() { return Boolean(hostController?.busy); },
    get autosaving() { return Boolean(timers.size); },
    identity, isCurrent, announce, intent, edit, connect, disconnect, scheduleObservation, startRecording,
    startHost: (mode, setting, options) => hostController?.start(mode, setting, options),
    updateHost: setting => hostController?.update(setting), stopHost,
    destroy() {
      if (destroying) return destroying;
      disposed = true;
      for (const timer of timers.values()) win.clearTimeout(timer);
      win.clearTimeout(observationTimer);
      hid.removeEventListener("disconnect", unplugged);
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
