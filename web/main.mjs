// SPDX-License-Identifier: GPL-3.0-or-later
import { mount } from "./app.mjs";
import { BackupStore } from "./storage.mjs";

const root = document.getElementById("app");
if (!globalThis.isSecureContext || !("hid" in navigator)) {
  root.textContent = "WebHID needs a secure context and a supported desktop browser. Open this page on HTTPS or localhost in Chrome or Edge.";
} else {
  try {
    const codec = await import("./pkg/byakko_web.js");
    await codec.default();
    mount(root, { codec, hid: navigator.hid, storage: new BackupStore() });
  } catch (error) {
    root.textContent = `Could not load Byakko: ${error.message ?? error}`;
  }
}
