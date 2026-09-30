// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, editable } from "./widgets.mjs";
import { createRuntime } from "./runtime.mjs";
import { createKeymapForm } from "./keymap-form.mjs";
import { createSettingsForm } from "./settings-form.mjs";
import { createMacroForm } from "./macro-form.mjs";
import { createLightingForm } from "./lighting-form.mjs";
import { createDiagnosticsForm } from "./diagnostics-form.mjs";
import { createDiscardDialog } from "./discard-dialog.mjs";
import { createBackups } from "./backups.mjs";
import { createKeyboard } from "./keyboard.mjs";
import { createRecorder } from "./recorder.mjs";
import { needsInitialRead } from "./settings_view.mjs";
import { createAppearance } from "./appearance.mjs";

// Composition, navigation and DOM focus belong here; features and effects have their own owners.
export function mount(root, {codec, hid, storage, now = () => performance.now(), wait, prepareHost} = {}) {
  if (!root || !codec?.BrowserSession || !codec?.BrowserOperation || !hid || !storage) {
    throw new Error("The browser session, WebHID and backup storage are required.");
  }
  const doc = root.ownerDocument, win = doc.defaultView ?? globalThis;
  let disposed = false, destroying = null, pendingRender = false, settingsAutoRead = false;
  const runtime = createRuntime({codec, hid, storage, now, wait, prepareHost, win,
    changed: background => render(background), isEditing: focusedForm,
    connected: (device, generation) => macros.connected(device, generation),
    disconnected: () => { settingsAutoRead = false; recorder.reset(); macros.disconnected(); },
  });
  let tab = "keymap", layer = runtime.view.descriptor.layers[0]?.id, key = runtime.view.descriptor.keys[0]?.id;
  const recorder = createRecorder({doc, win, runtime, now, redraw: render});
  const keymap = createKeymapForm({doc, runtime});
  const settings = createSettingsForm({doc, runtime});
  const discard = createDiscardDialog({doc, win});
  const macros = createMacroForm({doc, win, runtime, storage, recorder, redraw: render, navigate, confirmDiscard: discard.confirm});
  const lighting = createLightingForm({doc, win, runtime, redraw: render, navigate});
  const diagnostics = createDiagnosticsForm({doc, win, runtime});
  const backups = createBackups({doc, win, storage, runtime});
  const board = createKeyboard({doc, redraw: render,
    selectLayer: selected => { layer = selected; render(); }, selectKey: selected => { key = selected; }});
  root.replaceChildren();
  const appearance = createAppearance(doc, win);
  const shell = node(doc, "div", undefined, "shell");
  const header = node(doc, "header", undefined, "topbar");
  const brand = node(doc, "div", undefined, "brand");
  const mark = node(doc, "img", undefined, "brand-mark");
  mark.src = "assets/byakko.svg"; mark.alt = "";
  brand.append(mark, node(doc, "strong", "Byakko"), node(doc, "small", "Keyboard configurator"));
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
  header.append(brand, connection);
  main.append(status, keyboard, panel);
  workspace.append(sidebar, main);
  shell.append(header, workspace, discard.dialog, backups.dialog, diagnostics.dialog, appearance.dialog);
  root.append(shell);

  function focusedForm() {
    const active = doc.activeElement;
    return root.contains(active) && ["INPUT", "SELECT", "TEXTAREA"].includes(active?.tagName);
  }
  function navigate(selected) {
    if (selected !== "macros") void recorder.stop();
    tab = selected;
    if (selected === "settings") settingsAutoRead = true;
    render();
  }
  function maybeReadSettings() {
    const editor = runtime.view.settings?.editor;
    if (tab !== "settings" || !settingsAutoRead || !needsInitialRead(editor) || !editor.canRead ||
        !runtime.connection.device || disposed) return;
    settingsAutoRead = false;
    queueMicrotask(() => {
      if (runtime.connection.device && !disposed) void runtime.intent({ type: "read", feature: "settings" });
    });
  }
  function renderConnection() {
    const {device, connecting, closing, writes, listener, notificationStatus} = runtime.connection;
    connection.replaceChildren();
    const identity = node(doc, "span", device ? "Nia87 · USB connected" : "No keyboard connected", device ? "device-name connected" : "device-name");
    if (device) identity.title = `${device.productName || "Nia87"} · ${device.vendorId.toString(16)}:${device.productId.toString(16)}`;
    connection.append(identity);
    if (device) {
      const live = node(doc, "span", listener ? "Live updates" : notificationStatus || "Use Read to refresh", listener ? "live-status" : "live-status unavailable");
      live.title = notificationStatus; connection.append(live);
    } else connection.append(button(doc, connecting ? "Connecting…" : "Choose keyboard", () => runtime.connect(), Boolean(connecting || closing), "primary"));
    if (runtime.hostBusy) {
      const stop = button(doc, runtime.host.phase === "Stopping" ? "Restoring…" : "Stop host lighting", () => void lighting.stopHost(), closing || runtime.host.phase === "Stopping", "host-stop");
      stop.dataset.hostStop = "true";
      connection.append(stop);
    }
    if (device) connection.append(button(doc, "Disconnect", () => void runtime.disconnect(), Boolean(writes || closing || connecting)));
  }

  function renderSidebar(view) {
    appearance.button.disabled = view.recording;
    sidebar.replaceChildren(node(doc, "p", "Configure", "eyebrow"));
    for (const [id, label] of [["keymap", "Keymap"], ["macros", "Macros"], ["lighting", "Lighting"], ["settings", "Settings"], ["diagnostics", "Diagnostics"]]) {
      const dirty = editable(id === "macros" ? view.macros : view[id])?.dirty;
      const item = button(doc, `${label}${dirty ? " •" : ""}`, () => navigate(id), false, tab === id ? "nav-item active" : "nav-item");
      item.setAttribute("aria-current", tab === id ? "page" : "false"); sidebar.append(item);
    }
    const tools = node(doc, "div", undefined, "sidebar-tools");
    tools.append(appearance.button, button(doc, "View backups", () => void backups.show(), view.recording));
    sidebar.append(tools);
  }
  function render(background = false) {
    if (disposed) return;
    renderConnection();
    status.textContent = runtime.notice.message;
    status.classList.toggle("error", runtime.notice.error);
    // Preserve the real input and its unfinished value across asynchronous observations/completions.
    if (background && focusedForm()) pendingRender = true;
    else {
      pendingRender = false;
      const view = runtime.view, selection = {tab, layer, key};
      renderSidebar(view);
      board.render(keyboard, view, selection, macros.names, lighting);
      panel.replaceChildren();
      if (tab === "keymap") keymap.render(panel, view, selection, macros.names);
      else if (tab === "macros") macros.render(panel, view, selection);
      else if (tab === "lighting") lighting.render(panel, view, selection);
      else if (tab === "settings") settings.render(panel, view);
      else diagnostics.render(panel, view);
      if (runtime.host.phase === "Preparing") {
        for (const control of [...panel.querySelectorAll("button,input,select"), ...keyboard.querySelectorAll("button")]) {
          if (!control.dataset.hostStop) control.disabled = true;
        }
      }
    }
    maybeReadSettings();
  }
  const focusOut = () => { win.setTimeout(() => {
    if (disposed) return;
    if (pendingRender && !focusedForm()) render();
    runtime.scheduleObservation();
  }, 0); };
  const unloading = event => {
    const view = runtime.view;
    if (runtime.connection.writes || runtime.hostBusy || view.host?.phase !== "Idle" || macros.hasUnsavedNames ||
        [view.keymap, view.lighting?.editor, view.picture?.editor, view.macros?.editor, view.settings?.editor].some(item => item?.dirty)) {
      event.preventDefault(); event.returnValue = "";
    }
  };
  root.addEventListener("focusout", focusOut);
  win.addEventListener("beforeunload", unloading);
  render();
  return {
    get view() { return runtime.view; }, intent: runtime.intent, connect: runtime.connect, disconnect: runtime.disconnect,
    destroy() {
      if (destroying) return destroying;
      disposed = true;
      discard.destroy();
      recorder.destroy(); lighting.destroy(); backups.destroy(); appearance.destroy();
      root.removeEventListener("focusout", focusOut); win.removeEventListener("beforeunload", unloading);
      root.replaceChildren();
      destroying = runtime.destroy();
      return destroying;
    },
  };
}
