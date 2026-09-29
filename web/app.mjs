// SPDX-License-Identifier: GPL-3.0-or-later
import { filters, selectDevice, Reader } from "./transport.mjs";

const element = id => document.getElementById(id);
const connect = element("connect"), read = element("read"), disconnect = element("disconnect");
let reader = null;
let busy = false;
let choosing = false;
let closing = false;
let codec = null;
let observed = false;

function controls() {
  connect.disabled = !codec || busy || choosing || closing || reader !== null;
  read.disabled = reader === null || busy;
  disconnect.disabled = reader === null;
}

function clearObservation() {
  observed = false;
  read.textContent = "Read keyboard";
  element("lighting").textContent = "Read the selected keyboard to see its current lighting.";
  element("color").hidden = true;
  element("raw").textContent = "No observation yet.";
}

function show(result) {
  const lighting = result.lighting;
  const setting = lighting.setting;
  element("lighting").textContent = setting
    ? `Effect ${lighting.effectId} · ${lighting.effectName}${setting.value === null ? "" : ` · Brightness ${setting.value}/4`}`
    : `Unrecognized lighting values (effect ${lighting.effectId}). Raw bytes are preserved below.`;
  const rgb = setting?.rgb;
  element("color").hidden = !rgb;
  if (rgb) {
    element("swatch").style.backgroundColor = `rgb(${rgb.join(",")})`;
    element("rgb").textContent = `RGB ${rgb.join(", ")}${setting.dazzle ? " · Rainbow enabled" : ""}`;
  }
  element("raw").textContent = JSON.stringify(result, null, 2);
}

connect.addEventListener("click", async () => {
  choosing = true;
  controls();
  try {
    // Must be called directly from this user gesture, before any await.
    const devices = await navigator.hid.requestDevice({ filters });
    if (devices.length === 0) {
      element("status").textContent = "Selection cancelled. No keyboard opened.";
      return;
    }
    const device = selectDevice(devices);
    reader = new Reader(device, codec);
    clearObservation();
    element("device").textContent = `${device.productName || "Nia87"} · ${device.vendorId.toString(16)}:${device.productId.toString(16)}`;
    element("status").textContent = "Configuration interface selected. Ready to read.";
  } catch (error) {
    element("status").textContent = `Could not select keyboard: ${error.message ?? error}`;
  } finally {
    choosing = false;
    controls();
  }
});

read.addEventListener("click", async () => {
  const selected = reader;
  if (!selected || busy) return;
  busy = true;
  controls();
  element("status").textContent = "Reading identity, profile and lighting…";
  try {
    const result = await selected.readAll(observed);
    if (reader !== selected) return;
    show(result);
    observed = true;
    read.textContent = "Read again";
    element("status").textContent = "Read complete. Keyboard configuration unchanged.";
  } catch (error) {
    if (reader !== selected) return;
    element("status").textContent = `Read failed: ${error.message ?? error}${observed ? " Displayed values are from the previous read." : ""}`;
  } finally {
    busy = false;
    controls();
  }
});

async function detach(message) {
  const selected = reader;
  reader = null;
  clearObservation();
  element("device").textContent = "No keyboard selected.";
  element("status").textContent = `${message} Closing device access… If this stays pending, reload the page.`;
  closing = true;
  controls();
  try {
    await selected?.close();
    element("status").textContent = message;
  }
  catch (error) { element("status").textContent = `Connection ended; close failed: ${error.message ?? error}`; }
  finally { closing = false; controls(); }
}

disconnect.addEventListener("click", () => detach("Disconnected. Select the keyboard to reconnect."));

if (!globalThis.isSecureContext || !("hid" in navigator)) {
  element("status").textContent = "WebHID is unavailable. Open this preview over HTTPS or localhost in desktop Chrome or Edge.";
} else {
  navigator.hid.addEventListener("disconnect", event => {
    if (reader?.device === event.device) void detach("Keyboard unplugged. Select it again after reconnecting.");
  });
  try {
    codec = await import("./pkg/byakko_web.js");
    await codec.default();
    element("status").textContent = "Ready. Choose your keyboard to grant this page access.";
    controls();
  } catch (error) {
    codec = null;
    element("status").textContent = `Could not load the Rust module. Build the preview first. ${error.message ?? error}`;
  }
}
