// SPDX-License-Identifier: GPL-3.0-or-later
// Manual browser interaction fixture. Production main.mjs never imports this.
import * as codec from "./pkg/byakko_web.js";
import { mount } from "./app.mjs";
import { TestDevice } from "./test-device.mjs";
import { BackupStore } from "./storage.mjs";

await codec.default();
const device = new TestDevice();
const notifications = Object.assign(new EventTarget(), {
  vendorId: device.vendorId, productId: device.productId, opened: false,
  collections: [{usagePage: 0xffff, usage: 1, children: [], inputReports: [{reportId:5, items:[{reportSize:8,reportCount:3}]}]}],
  async open() { this.opened = true; }, async close() { this.opened = false; },
});
function notify(payload) {
  notifications.dispatchEvent(Object.assign(new Event("inputreport"), {
    device: notifications, reportId: 5, data: new DataView(Uint8Array.from(payload).buffer),
  }));
}
const hid = new EventTarget();
hid.requestDevice = async () => [device, notifications];
const storage = new BackupStore({ open: (_, version) => indexedDB.open("byakko-browser-tests", version) });
const app = mount(document.getElementById("app"), {
  codec, hid, storage, wait: async ms => device.calls.push({ kind: "wait", ms }),
  prepareHost: async source => {
    let timer;
    return {label:"Synthetic browser capture",start(receive) {
      timer = setInterval(() => receive(source === "ScreenAverage" ? {Rgb:[24,48,96]} : {Bands:Array(32).fill(4)}), 100);
    },async stop(){clearInterval(timer);}};
  },
});
document.getElementById("onboard-lighting").addEventListener("click", () => {
  device.lighting[3] = device.lighting[3] === 2 ? 3 : 2;
  notify([6, device.lighting[3], 0]);
});
document.getElementById("onboard-settings").addEventListener("click", () => {
  notify([3, 1, 9]);
});

document.getElementById("fail-lighting-read").addEventListener("click", () => {
  device.beforeReceive = request => {
    if (request[0] === 0x87) {
      device.beforeReceive = null;
      throw new Error("Simulated lighting read failure");
    }
  };
});
document.getElementById("inspect").addEventListener("click", async () => {
  const backups = await storage.all();
  document.getElementById("evidence").textContent = JSON.stringify({
    backups: backups.length,
    writes: device.calls.filter(call => call.kind === "send" && call.report[0] < 0x80 && ![0x0d,0x0e].includes(call.report[0])).map(call => call.report),
    hostFrames: device.calls.filter(call => call.kind === "send" && [0x0d,0x0e].includes(call.report[0])).length,
    reads: device.calls.filter(call => call.kind === "receive").length,
    keymapDirty: app.view.keymap.dirty,
    macroDirty: app.view.macros.editor.dirty,
    lightingDirty: app.view.lighting.editor.dirty,
    pictureDirty: app.view.picture.editor.dirty,
    settingsDirty: app.view.settings.editor.dirty,
    lightingBrightness: app.view.lighting.editor.draft?.brightness,
    observation: app.view.observation,
    settings: app.view.settings.editor.draft,
    host: app.view.host,
    archive: app.view.archive,
  }, null, 2);
});
