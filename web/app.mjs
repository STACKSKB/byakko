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
import { transportLabel } from "./transport.mjs";
import { editorAttention } from "./editor-attention.mjs";

// Composition, navigation and DOM focus belong here; features and effects have their own owners.
export function mount(root, {codec, hid, storage, now = () => performance.now(), wait, prepareHost} = {}) {
  if (!root || !codec?.BrowserSession || !codec?.BrowserOperation || !hid || !storage) {
    throw new Error("The browser session, WebHID and backup storage are required.");
  }
  const doc = root.ownerDocument, win = doc.defaultView ?? globalThis;
  let disposed = false, destroying = null, settingsAutoRead = false;
  const runtime = createRuntime({codec, hid, storage, now, wait, prepareHost, win,
    changed: () => render(), isEditing: focusedForm,
    connected: (device, generation) => macros.connected(device, generation),
    disconnected: () => { settingsAutoRead = false; recorder.reset(); macros.disconnected(); },
  });
  let tab = "keymap", renderedTarget = null, layer = runtime.view.descriptor.layers[0]?.id, key = runtime.view.descriptor.keys[0]?.id;
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
  let connectionParts = null;
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
    if (!connectionParts) {
      const identity = node(doc, "span", undefined, "device-name");
      const live = node(doc, "span", undefined, "live-status");
      const choose = button(doc, "Choose keyboard", () => runtime.connect(), false, "primary");
      const stop = button(doc, "Stop host lighting", () => void lighting.stopHost(), false, "host-stop"); stop.dataset.hostStop = "true";
      const disconnect = button(doc, "Disconnect", () => void runtime.disconnect());
      connectionParts = {identity, live, choose, stop, disconnect};
      connection.append(identity);
    }
    const {identity, live, choose, stop, disconnect} = connectionParts;
    identity.textContent = device ? `Nia87 · ${transportLabel(device)} connected` : "No keyboard connected";
    identity.classList.toggle("connected", Boolean(device));
    identity.title = device ? `${device.productName || "Nia87"} · ${device.vendorId.toString(16)}:${device.productId.toString(16)}` : "";
    live.textContent = listener ? "Live updates" : notificationStatus || "Use Read to refresh";
    live.classList.toggle("unavailable", !listener);
    live.title = notificationStatus;
    choose.textContent = connecting ? "Connecting…" : "Choose keyboard";
    choose.disabled = Boolean(connecting || closing);
    stop.textContent = runtime.host.phase === "Stopping" ? "Restoring…" : "Stop host lighting";
    stop.disabled = closing || runtime.host.phase === "Stopping";
    disconnect.disabled = Boolean(writes || closing || connecting);
    if (device) {
      choose.remove();
      if (!live.isConnected) connection.append(live);
      if (!disconnect.isConnected) connection.append(disconnect);
    } else {
      live.remove(); disconnect.remove();
      if (!choose.isConnected) connection.append(choose);
    }
    if (runtime.hostBusy) {
      if (!stop.isConnected) connection.insertBefore(stop, disconnect.isConnected ? disconnect : null);
    } else stop.remove();
  }

  function renderSidebar(view) {
    appearance.button.disabled = view.recording;
    if (!sidebar.hasChildNodes()) {
      sidebar.append(node(doc, "p", "Configure", "eyebrow"));
      for (const id of ["keymap", "macros", "lighting", "settings", "diagnostics"]) {
        const item = button(doc, "", () => navigate(id), false, "nav-item");
        item.dataset.feature = id;
        sidebar.append(item);
      }
      const tools = node(doc, "div", undefined, "sidebar-tools");
      const backupsButton = button(doc, "View backups", () => void backups.show());
      backupsButton.dataset.backups = "true";
      tools.append(appearance.button, backupsButton);
      sidebar.append(tools);
    }
    for (const [id, label] of [["keymap", "Keymap"], ["macros", "Macros"], ["lighting", "Lighting"], ["settings", "Settings"], ["diagnostics", "Diagnostics"]]) {
      const dirty = editable(id === "macros" ? view.macros : view[id])?.dirty;
      const item = [...sidebar.querySelectorAll(".nav-item")].find(element => element.dataset.feature === id);
      item.textContent = `${label}${dirty ? " •" : ""}`;
      item.classList.toggle("active", tab === id);
      item.setAttribute("aria-current", tab === id ? "page" : "false");
    }
    sidebar.querySelector("[data-backups]").disabled = view.recording;
  }
  function renderAttention(target, view) {
    const features = tab === "lighting" ? ["lighting", "picture"] : tab === "macros" ? ["macros", "keymap"] : [tab];
    const messages = features.flatMap(feature => {
      const message = editorAttention(editable(view[feature]), feature);
      return message ? [message] : [];
    });
    let attention = target.querySelector(".editor-attention");
    if (!messages.length) { attention?.remove(); return; }
    if (!attention) {
      attention = node(doc, "div", undefined, "editor-attention");
      attention.setAttribute("role", "alert");
      target.insertBefore(attention, target.children[2] ?? null);
    }
    attention.replaceChildren(...messages.map(message => node(doc, "p", message)));
  }
  function controlIdentity(control) {
    if (control.tagName === "BUTTON") {
      if (control.dataset.key) return `KEY:${control.dataset.key}`;
      if (control.dataset.featureAction) return `ACTION:${control.dataset.featureAction}`;
      if (control.classList.contains("host-stop")) return "ACTION:host-stop";
      return control.textContent ? `BUTTON:${control.textContent}` : null;
    }
    const label = control.getAttribute("aria-label") ?? control.closest("label.field")?.querySelector("span")?.textContent
      ?? control.closest("label")?.textContent ?? control.getAttribute("placeholder");
    if (!label) return null;
    return `${control.tagName}:${control.type}:${label.replace(/ · \d+$/, "")}`;
  }
  function preserveActiveControl(oldRegion, newRegion, active) {
    if (!oldRegion.contains(active)) return false;
    const identity = controlIdentity(active);
    if (!identity) return false;
    const matches = [...newRegion.querySelectorAll("input,select,textarea,button")].filter(item => controlIdentity(item) === identity);
    if (matches.length !== 1) return false;
    const counterpart = matches[0];
    const path = (element, root) => {
      const ancestors = [];
      while (element !== root) { ancestors.unshift(element); element = element.parentNode; }
      return ancestors;
    };
    const current = path(active, oldRegion), updated = path(counterpart, newRegion);
    if (current.length !== updated.length || current.some((element, index) => element.tagName !== updated[index].tagName)) return false;
    let oldParent = oldRegion, newParent = newRegion;
    for (let index = 0; index < current.length; index++) {
      const oldChild = current[index], newChild = updated[index];
      const oldSiblings = [...oldParent.childNodes].filter(node => node !== oldChild);
      const newSiblings = [...newParent.childNodes];
      const pivot = newSiblings.indexOf(newChild);
      for (const sibling of newSiblings.slice(0, pivot)) oldParent.insertBefore(sibling, oldChild);
      const after = oldChild.nextSibling;
      for (const sibling of newSiblings.slice(pivot + 1)) oldParent.insertBefore(sibling, after);
      for (const sibling of oldSiblings) sibling.remove();
      oldChild.className = newChild.className;
      oldParent = oldChild; newParent = newChild;
    }
    active.disabled = counterpart.disabled;
    if (active.tagName === "BUTTON") {
      active.replaceChildren(...counterpart.childNodes);
      active.title = counterpart.title;
      active.style.cssText = counterpart.style.cssText;
      for (const name of ["aria-label", "aria-pressed"]) {
        if (counterpart.hasAttribute(name)) active.setAttribute(name, counterpart.getAttribute(name));
        else active.removeAttribute(name);
      }
    }
    if (active.type === "checkbox" || active.type === "radio") active.checked = counterpart.checked;
    for (const name of ["min", "max", "step", "aria-label"]) {
      if (counterpart.hasAttribute(name)) active.setAttribute(name, counterpart.getAttribute(name));
      else active.removeAttribute(name);
    }
    if (active.tagName === "SELECT") {
      if (active.options.length !== counterpart.options.length || [...active.options].some((option, index) =>
          option.value !== counterpart.options[index]?.value || option.textContent !== counterpart.options[index]?.textContent)) {
        active.replaceChildren(...counterpart.childNodes);
      }
      active.value = counterpart.value;
    }
    return true;
  }
  function render() {
    if (disposed) return;
    status.textContent = runtime.notice.message;
    status.classList.toggle("error", runtime.notice.error);
    const view = runtime.view, selection = {tab, layer, key};
    const active = doc.activeElement;
    const keyboardScroll = keyboard.querySelector(".keyboard-scroll")?.scrollLeft ?? 0;
    const sidebarScroll = sidebar.scrollTop;
    const pageScroll = doc.scrollingElement?.scrollTop;
    const openDetails = new Set([...panel.querySelectorAll("details[open] summary")].map(summary => summary.textContent));
    const listScroll = panel.querySelector(".slot-list")?.scrollTop ?? 0;
    const nextKeyboard = node(doc, "div");
    const nextPanel = node(doc, "section");
    board.render(nextKeyboard, view, selection, macros.names, lighting);
    if (tab === "keymap") keymap.render(nextPanel, view, selection, macros.names);
    else if (tab === "macros") macros.render(nextPanel, view, selection);
    else if (tab === "lighting") lighting.render(nextPanel, view, selection);
    else if (tab === "settings") settings.render(nextPanel, view);
    else diagnostics.render(nextPanel, view);
    renderAttention(nextPanel, view);
    for (const details of nextPanel.querySelectorAll("details")) {
      if (openDetails.has(details.querySelector("summary")?.textContent)) details.open = true;
    }
    const target = {tab, layer, key, slot: view.macros?.slot, generation: view.connection.generation, connection: view.connection.kind};
    const sameTarget = renderedTarget && Object.keys(target).every(field => target[field] === renderedTarget[field]);
    const sameKeyboardTarget = renderedTarget && ["tab", "layer", "generation", "connection"]
      .every(field => target[field] === renderedTarget[field]);
    const keptKeyboard = sameKeyboardTarget && preserveActiveControl(keyboard, nextKeyboard, active);
    const keptPanel = !keptKeyboard && sameTarget && preserveActiveControl(panel, nextPanel, active);
    renderConnection();
    renderSidebar(view);
    if (!keptKeyboard) keyboard.replaceChildren(...nextKeyboard.childNodes);
    if (!keptPanel) panel.replaceChildren(...nextPanel.childNodes);
    renderedTarget = target;
    keyboard.querySelector(".keyboard-scroll").scrollLeft = keyboardScroll;
    sidebar.scrollTop = sidebarScroll;
    if (panel.querySelector(".slot-list")) panel.querySelector(".slot-list").scrollTop = listScroll;
    if (runtime.host.phase === "Preparing") {
      for (const control of [...panel.querySelectorAll("button,input,select"), ...keyboard.querySelectorAll("button")]) {
        if (!control.dataset.hostStop) control.disabled = true;
      }
    }
    if (pageScroll !== undefined) doc.scrollingElement.scrollTop = pageScroll;
    maybeReadSettings();
  }
  const focusOut = () => { win.setTimeout(() => {
    if (disposed) return;
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
