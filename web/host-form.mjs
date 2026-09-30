// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, field, choice, number, range, hex, rgb, text } from "./widgets.mjs";

export function createHostForm({ doc, win, runtime, redraw, navigate }) {
  let hostMode = null, hostSetting = null, hostUpdateTimer = null;
  let screenSampling = "average", screenX = 500, screenY = 500;

  function selectedMode(view) {
    const modes = view.lighting?.capabilities?.host_modes ?? [];
    return modes.find(mode => mode.id === hostMode) ?? modes[0];
  }

  function clearUpdate() {
    win.clearTimeout(hostUpdateTimer);
    hostUpdateTimer = null;
  }

  function select(id) {
    clearUpdate();
    hostMode = id;
    hostSetting = null;
  }

  function currentSetting(mode) {
    if (!mode?.parameters) return null;
    if (hostSetting?.effect !== mode.id) hostSetting = structuredClone(mode.parameters.default);
    return hostSetting;
  }

  function queueUpdate() {
    clearUpdate();
    if (runtime.host.phase !== "Active") return;
    const identity = runtime.identity();
    hostUpdateTimer = win.setTimeout(() => {
      hostUpdateTimer = null;
      if (!runtime.isCurrent(identity) || runtime.host.phase !== "Active") return;
      const setting = structuredClone(hostSetting);
      void runtime.updateHost(setting).catch(error => runtime.announce(`Host lighting parameters: ${text(error)}`, true));
    }, 120);
  }

  function changeSetting(change) {
    hostSetting = { ...hostSetting, ...change };
    queueUpdate();
    redraw();
  }

  async function start() {
    const mode = selectedMode(runtime.view);
    if (!mode) return;
    const setting = currentSetting(mode);
    const options = mode.source === "ScreenAverage"
      ? { sampling: screenSampling, x: screenX, y: screenY } : {};
    try { await runtime.startHost(mode, setting, options); }
    catch (error) { runtime.announce(`Host lighting: ${text(error)}`, true); }
  }

  async function stop() {
    clearUpdate();
    await runtime.stopHost();
  }

  function render(panel, view) {
    const modes = view.lighting?.capabilities?.host_modes ?? [];
    if (!modes.length) return;
    const mode = selectedMode(view);
    const host = runtime.host;
    const editor = view.lighting.editor;
    const settings = view.settings?.editor;
    const required = mode.requires_enabled_setting;
    const enabled = !required || (settings?.status === "Ready" && !settings.dirty &&
      settings.baseline?.content?.Editable?.[required]?.Toggle === true);
    const clean = editor.status === "Ready" && Boolean(editor.draft) && !editor.dirty;
    const canStart = Boolean(runtime.connection.device && host.phase === "Idle" && view.host?.phase === "Idle" &&
      !view.busy && !view.recording && !runtime.autosaving && clean && enabled);
    const card = node(doc, "div", undefined, "card host-card");
    card.append(node(doc, "h3", "Host lighting"));
    card.append(node(doc, "p", "Capture stays local to this browser. Samples are sent only to the selected keyboard. Stop to restore its original onboard lighting.", "muted"));
    card.append(node(doc, "h3", mode.label));
    if (mode.source === "ScreenAverage") {
      field(doc, card, "Screen sample", choice(doc, [["average", "Average color"], ["point", "Selected point"]], screenSampling,
        value => { screenSampling = value; redraw(); }, host.phase !== "Idle"));
      if (screenSampling === "point") {
        const point = node(doc, "div", undefined, "host-point");
        for (const [label, coordinate, update] of [["X", screenX, value => { screenX = value; }], ["Y", screenY, value => { screenY = value; }]]) {
          const input = number(doc, coordinate, 0, 1000, value => {
            if (Number.isInteger(value) && value >= 0 && value <= 1000) update(value);
            else runtime.announce("Choose a screen point from 0 to 1000.", true);
          }, host.phase !== "Idle");
          field(doc, point, `${label} · 0–1000`, input);
        }
        card.append(point);
      }
      card.append(node(doc, "p", "Choose a tab, window or screen in the browser share dialog.", "muted"));
    } else card.append(node(doc, "p", "Choose a tab or screen with Share audio enabled. No microphone fallback is used.", "muted"));
    if (mode.parameters) {
      const value = currentSetting(mode), schema = mode.parameters.schema;
      const params = node(doc, "div", undefined, "host-parameters");
      if (schema.brightness) {
        const [min, max] = range(schema.brightness);
        const slider = node(doc, "input");
        slider.type = "range"; slider.min = String(min); slider.max = String(max); slider.value = String(value.brightness);
        slider.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping";
        slider.addEventListener("change", () => changeSetting({ brightness: Number(slider.value) }));
        field(doc, params, `Brightness · ${value.brightness}`, slider);
      }
      if (schema.options.length) field(doc, params, "Style", choice(doc, schema.options.map(item => [item.id, item.label]),
        value.option, option => changeSetting({ option }), host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping"));
      if (schema.color) {
        if (schema.color !== "Rainbow") {
          const picker = node(doc, "input"); picker.type = "color";
          picker.value = value.color?.Rgb ? hex(value.color.Rgb) : "#ffffff";
          picker.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping" || value.color === "Rainbow";
          picker.addEventListener("change", () => changeSetting({ color: { Rgb: rgb(picker.value) } }));
          field(doc, params, "Color", picker);
        }
        if (schema.color === "FixedOrRainbow" || schema.color === "Rainbow") {
          const label = node(doc, "label", "Rainbow", "host-rainbow");
          const toggle = node(doc, "input"); toggle.type = "checkbox"; toggle.checked = value.color === "Rainbow";
          toggle.disabled = host.phase === "Preparing" || host.phase === "Starting" || host.phase === "Stopping";
          toggle.addEventListener("change", () => changeSetting({ color: toggle.checked ? "Rainbow" : { Rgb: [255, 255, 255] } }));
          label.prepend(toggle); params.append(label);
        }
      }
      card.append(params);
    }
    if (!enabled) {
      card.append(node(doc, "p", "Read Settings, enable and save Backlight, then return here.", "muted"));
      card.append(button(doc, "Open Settings", () => navigate("settings")));
    } else if (!clean && host.phase === "Idle") card.append(node(doc, "p", "Read lighting and save or revert its draft before starting capture.", "muted"));
    const controls = node(doc, "div", undefined, "actions");
    controls.append(button(doc, host.phase === "Preparing" ? "Choose capture…" : "Start host lighting", () => void start(), !canStart, "primary"));
    if (host.phase !== "Idle") {
      const stopButton = button(doc, host.phase === "Stopping" ? "Restoring…" : "Stop and restore", () => void stop(), host.phase === "Stopping", "host-stop");
      stopButton.dataset.hostStop = "true";
      controls.append(stopButton);
    }
    card.append(controls);
    if (host.phase !== "Idle") card.append(node(doc, "p", `${host.phase}${host.source ? ` · ${host.source}` : ""}`, "host-phase"));
    if (host.problem) card.append(node(doc, "p", host.problem, "error"));
    panel.append(card);
  }

  return { select, selectedMode, render, stop, destroy: clearUpdate };
}
