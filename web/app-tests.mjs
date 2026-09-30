// SPDX-License-Identifier: GPL-3.0-or-later
// Real DOM event handlers + real WASM, with deferred local effects and fake HID.
import * as codec from "./pkg/byakko_web.js";
import { mount } from "./app.mjs";
import { TestDevice, TestStore } from "./test-device.mjs";
await codec.default();

const tick = () => new Promise(resolve => setTimeout(resolve, 0));
function assert(condition, message) { if (!condition) throw new Error(message); }
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
async function until(predicate) {
  const end = performance.now() + 4000;
  while (!predicate()) { assert(performance.now() < end, "Timed out waiting for UI state"); await tick(); }
}
function button(root, text) {
  const found = [...root.querySelectorAll("button")].find(element => element.textContent === text);
  assert(found && !found.disabled, `Missing enabled button: ${text}`);
  return found;
}
function nameInput(root, value) {
  const input = root.querySelector('input[placeholder="Local macro name"]');
  assert(input, "Missing name field");
  if (value !== undefined) { input.value = value; input.dispatchEvent(new Event("input", {bubbles:true})); }
  return input;
}
async function harness(run) {
  const root = document.getElementById("fixture");
  const device = new TestDevice();
  const storage = new TestStore();
  storage.macroNames = async () => ({});
  storage.saveMacroName = async () => {};
  const notifications = Object.assign(new EventTarget(), {
    vendorId: device.vendorId, productId: device.productId, opened: false,
    collections: [{usagePage:0xffff, usage:1, children:[], inputReports:[{reportId:5, items:[{reportSize:8,reportCount:3}]}]}],
    async open() { this.opened = true; }, async close() { this.opened = false; },
  });
  const hid = new EventTarget();
  hid.requestDevice = async () => [device, notifications];
  const app = mount(root, {codec, hid, storage, wait: async () => {}});
  async function connect() {
    await app.connect();
    await until(() => !app.view.macros.discoveryBusy);
    await app.intent({type:"read", feature:"macro"});
    button(root, "Macros").click();
  }
  const notify = () => notifications.dispatchEvent(Object.assign(new Event("inputreport"), {
    device:notifications, reportId:5, data:new DataView(Uint8Array.from([6,3,0]).buffer),
  }));
  try { await connect(); await run({root, app, device, storage, connect, notify}); }
  finally { await app.destroy(); }
}
async function documentFor(app) {
  await app.intent({type:"initializeMacro"});
  const result = await app.intent({type:"exportMacroDocument", name:"Imported name", binding:null});
  assert(result.ok, result.error);
  await app.intent({type:"revert", feature:"macro"});
  return JSON.stringify(result.outcome.document);
}
function beginImport(root, pending) {
  const input = root.querySelector('input[type="file"]');
  assert(input, "Missing macro import control");
  Object.defineProperty(input, "files", {value:[{size:100, text:() => pending.promise}]});
  input.dispatchEvent(new Event("change", {bubbles:true}));
}

const tests = [
  ["a current import still stages its document and name", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending); pending.resolve(contents);
    await until(() => h.app.view.macros.editor.dirty);
    assert(nameInput(h.root).value === "Imported name", "Import metadata missing");
  }],
  ["an import cannot follow a slot change", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending);
    await h.app.intent({type:"selectMacro", slot:"slot-01"});
    await h.app.intent({type:"read", feature:"macro"});
    pending.resolve(contents); await tick();
    assert(h.app.view.macros.slot === "slot-01" && !h.app.view.macros.editor.dirty, "Import modified the new slot");
    assert(nameInput(h.root).value === "", "Import leaked metadata");
  }],
  ["an import cannot follow reconnecting the same HID object", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending);
    await h.app.disconnect(); await h.connect();
    pending.resolve(contents); await tick();
    assert(!h.app.view.macros.editor.dirty, "Import modified the new session");
    assert(nameInput(h.root).value === "", "Import leaked a name across reconnect");
  }],
  ["name-save completion preserves newer typing", async h => {
    const pending = deferred(); h.storage.saveMacroName = () => pending.promise;
    nameInput(h.root, "Submitted"); button(h.root, "Save name").click();
    nameInput(h.root, "Newer draft"); pending.resolve(); await tick();
    assert(nameInput(h.root).value === "Newer draft", "Save discarded newer name text");
  }],
  ["name-save success cannot leak across reconnect", async h => {
    const pending = deferred(); h.storage.saveMacroName = () => pending.promise;
    nameInput(h.root, "Old session"); button(h.root, "Save name").click();
    await h.app.disconnect(); await h.connect();
    pending.resolve(); await tick();
    assert(nameInput(h.root).value === "", "Old save changed the reconnected UI");
  }],
  ["name-save failure cannot overwrite a reconnected notice", async h => {
    const pending = deferred(); h.storage.saveMacroName = () => pending.promise;
    nameInput(h.root, "Old session"); button(h.root, "Save name").click();
    await h.app.disconnect(); await h.connect();
    const notice = h.root.querySelector('.status').textContent;
    pending.reject(new Error("old write failed")); await tick();
    assert(h.root.querySelector('.status').textContent === notice, "Old error replaced the current notice");
  }],
  ["onboard notifications retain unfinished numeric edits and defer reads", async h => {
    button(h.root, "Lighting").click();
    const input = h.root.querySelector('input[type="number"]');
    input.focus(); input.value = "2";
    const reads = () => h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x87).length;
    const before = reads();
    h.notify();
    assert(input.isConnected && document.activeElement === input && input.value === "2", "Notification replaced the active edit");
    await new Promise(resolve => setTimeout(resolve, 600));
    assert(reads() === before && h.app.view.observation.queued, "Notification bypassed the editing gate");
    input.dispatchEvent(new Event("change", {bubbles:true}));
    assert(h.app.view.lighting.editor.draft.brightness === 2, "Retained input could not commit");
    await until(() => !h.app.view.observation.queued && !h.app.view.busy && !h.app.view.lighting.editor.dirty);
    assert(reads() > before, "Observation did not resume after editing");
  }],
  ["background read completion retains input focus and flushes after leaving", async h => {
    const pending = deferred(); let entered = false;
    h.device.beforeReceive = request => {
      if (request[0] === 0x87) { entered = true; return pending.promise; }
    };
    const input = nameInput(h.root, "Unfinished name"); input.focus();
    await h.app.intent({type:"read", feature:"lighting"}, true);
    await until(() => entered);
    pending.resolve(); await until(() => !h.app.view.busy);
    assert(input.isConnected && document.activeElement === input && input.value === "Unfinished name", "Completion replaced the active edit");
    document.getElementById("heading").focus();
    await tick();
    assert(!input.isConnected && nameInput(h.root).value === "Unfinished name", "Deferred render did not flush with the draft intact");
  }],
];

document.getElementById("run").addEventListener("click", async event => {
  event.target.disabled = true;
  const output = document.getElementById("results");
  output.textContent = "";
  let passed = 0;
  for (const [name, run] of tests) {
    try { await harness(run); passed++; output.textContent += `PASS ${name}\n`; }
    catch (error) { output.textContent += `FAIL ${name}: ${error.message}\n`; }
  }
  output.textContent += `${passed}/${tests.length} passed`;
  event.target.disabled = false;
});
document.getElementById("results").textContent = "Ready";
document.getElementById("run").disabled = false;
