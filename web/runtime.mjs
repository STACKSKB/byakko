// SPDX-License-Identifier: GPL-3.0-or-later
import { selectionFilters, selectDevice } from "./transport.mjs";
import { DeviceExecutor } from "./executor.mjs";
import { notificationDevice, NotificationListener } from "./notifications.mjs";
import { HostController } from "./host.mjs";
import { editable, text } from "./widgets.mjs";
import { assignmentFailureMessage, assignmentSuccessMessage, deviceProblemMessage, workflowProblemMessage } from "./outcome-message.mjs";

// Owns one connection and its ordered effects. Editable feature values stay in core.
export function createRuntime({ codec, hid, storage, now, wait, prepareHost, win,
  changed, isEditing, connected, disconnected }) {
  const session = new codec.BrowserSession();
  let view = JSON.parse(session.view());
  let device = null, executor = null, connecting = false, closing = false;
  let notice = "Choose a Nia87 keyboard.", noticeError = false, noticeKind = null;
  let writes = 0, disposed = false, previousDiscovery = false, destroying = null, connectTask = null;
  let listener = null, notificationStatus = "", generation = null, observationTimer = null;
  let interactionUntil = 0, hostController = null;
  const timers = new Map(), inflight = new Set();

  function handle(response, background = false) {
    const previousCatalogError = Boolean(view.macros?.discoveryError);
    view = response.view;
    noticeKind = response.outcome?.kind ?? null;
    executor?.setRecording(view.recording);
    if (response.outcome?.cancelCatalog) executor?.cancelCatalog();
    const discovery = Boolean(view.macros?.discoveryBusy);
    if (previousDiscovery && !discovery) executor?.cancelCatalog();
    previousDiscovery = discovery;
    if (!response.ok) { notice = response.error; noticeError = true; }
    else if (response.outcome?.kind === "hostFailed") {
      noticeError = true;
      notice = "Host lighting restoration failed. Read Lighting before retrying.";
    }
    else if (response.outcome?.kind === "archiveCaptureFailed") {
      noticeError = true;
      notice = "Diagnostic capture failed. Retry Capture.";
    }
    else if (response.outcome?.kind === "assignmentFailed") {
      noticeError = true;
      notice = assignmentFailureMessage(response.outcome.detail);
    }
    else if (response.outcome?.kind === "failed") {
      noticeError = true;
      notice = deviceProblemMessage(response.outcome.detail);
    }
    else if (response.outcome?.kind === "picturePreparationFailed") {
      noticeError = true;
      notice = response.outcome.detail.lightingApplied
        ? "Lighting mode changed; per-key colors did not load. Read Lighting."
        : `Per-key colors did not load. ${workflowProblemMessage(response.outcome.detail.problem)}`;
    }
    else if (response.outcome?.kind === "catalogFailed") {
      noticeError = true;
      notice = "Macro discovery failed. Retry discovery.";
    } else if (response.outcome?.kind === "catalogLoaded" && previousCatalogError) {
      noticeError = false;
      notice = "";
    } else if (response.outcome?.kind === "conflict") { noticeError = true; notice = "Keyboard changed. Draft kept; Revert, then Read."; }
    else {
      const saved = {
        saved: "Keymap saved (read back).",
        macroSaved: "Macro saved (read back).",
        lightingSaved: "Lighting sent (not read back).",
        pictureSaved: "Per-key colors sent (not read back).",
        settingsSaved: "Settings saved (read back).",
        assignmentSucceeded: assignmentSuccessMessage(response.outcome?.detail),
        archiveCaptured: "Diagnostic archive captured.",
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
    if (device?.productId === 0x4011 && ["hostStart", "hostUpdate"].includes(input.type)) {
      noticeError = true;
      noticeKind = null;
      notice = "Host lighting over the 2.4 GHz receiver is not verified.";
      changed(background);
      return { ok: false, error: notice, view };
    }
    const result = handle(JSON.parse(session.dispatch(JSON.stringify(input))), background);
    if (result.ok && !result.command) {
      const messages = {
        edit: ["lighting", "picture", "settings"].includes(input.feature)
          ? "Autosaving in 500 ms…"
          : "Draft changed. Save when ready.",
        revert: "Draft reverted.",
        initializeMacro: "Macro ready for events.",
        importMacroDocument: "Macro staged. Save when ready.",
        recordStart: "Recording locally. Stop when done.",
        recordStop: "Recording stopped. Review before saving.",
      };
      if (messages[input.type]) { noticeError = false; noticeKind = null; notice = messages[input.type]; changed(); }
    }
    if (result.command) {
      const running = execute(result.command).catch(error => {
        noticeError = true;
        noticeKind = null;
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
      executor = new DeviceExecutor(device, codec.BrowserOperation, storage, wait, codec);
      const result = handle(JSON.parse(session.dispatch('{"type":"connect"}')));
      if (!result.ok) throw new Error(result.error);
      generation = result.outcome.generation;
      await executor.verifyReceiver();
      if (disposed) return;
      hostController = new HostController({
        codec, dispatch: input => intent(input), executor,
        changed: state => {
          noticeError = Boolean(state.problem);
          noticeKind = null;
          if (state.problem) notice = `Host lighting: ${state.problem}`;
          else if (state.phase === "Active") notice = "Host lighting active. Stop to restore onboard lighting.";
          else if (state.phase === "Idle" && view.host?.phase === "Idle") notice = "Onboard lighting restored.";
          changed(true);
          scheduleObservation();
        },
        prepare: prepareHost, environment: win,
      });
      const selectedDevice = device;
      const selectedGeneration = generation;
      try {
        if (device.productId === 0x4011) {
          notificationStatus = "Receiver onboard updates are unverified. Use Read to refresh.";
        } else {
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
        }
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
      if (device.productId !== 0x4011 || view.keymap.status === "Ready") {
        await intent({ type: "read", feature: "lighting" });
      }
      if (disposed || device !== selectedDevice) return;
      const ready = view.keymap.status === "Ready" && view.lighting?.editor.status === "Ready";
      if (ready) { noticeError = false; notice = "Keyboard ready."; }
      else if (!noticeError) notice = "Some keyboard data could not be read. Use the feature Read control to retry.";
      changed();
      if (!disposed && (device.productId !== 0x4011 || ready)) void intent({ type: "discoverMacros" }, true);
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
    notice = message; noticeError = error; noticeKind = null; changed(background);
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
    get notice() { return { message: notice, error: noticeError, kind: noticeKind }; },
    get host() { return hostController?.state ?? { phase: "Idle", source: "", problem: null }; },
    get hostBusy() { return Boolean(hostController?.busy); },
    get autosaving() { return Boolean(timers.size); },
    get hostModesAvailable() { return device?.productId === 0x4015; },
    identity, isCurrent, announce, intent, edit, connect, disconnect, scheduleObservation, startRecording,
    startHost: (mode, setting, options) => device?.productId === 0x4011
      ? Promise.reject(new Error("Host lighting over the 2.4 GHz receiver is not verified."))
      : hostController?.start(mode, setting, options),
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
