// SPDX-License-Identifier: GPL-3.0-or-later
// Manual browser interaction fixture. Production main.mjs never imports this.
import * as codec from "./pkg/byakko_web.js";
import { mount } from "./app.mjs";
import { TestDevice } from "./test-device.mjs";
import { BackupStore } from "./storage.mjs";

await codec.default();
const device = new TestDevice();
const hid = new EventTarget();
hid.requestDevice = async () => [device];
const storage = new BackupStore({ open: (_, version) => indexedDB.open("byakko-browser-tests", version) });
const app = mount(document.getElementById("app"), {
  codec, hid, storage, wait: async ms => device.calls.push({ kind: "wait", ms }),
});
document.getElementById("inspect").addEventListener("click", async () => {
  const backups = await storage.all();
  document.getElementById("evidence").textContent = JSON.stringify({
    backups: backups.length,
    writes: device.calls.filter(call => call.kind === "send" && call.report[0] < 0x80).map(call => call.report),
    reads: device.calls.filter(call => call.kind === "receive").length,
    keymapDirty: app.view.keymap.dirty,
    macroDirty: app.view.macros.editor.dirty,
    lightingDirty: app.view.lighting.editor.dirty,
    pictureDirty: app.view.picture.editor.dirty,
  }, null, 2);
});
