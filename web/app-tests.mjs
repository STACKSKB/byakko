// SPDX-License-Identifier: GPL-3.0-or-later
// Real DOM event handlers + real WASM, with deferred local effects and fake HID.
import * as codec from "./pkg/byakko_web.js";
import { createDiscardDialog } from "./discard-dialog.mjs";
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
  const found = [...root.querySelectorAll("button")].find(element => element.textContent === text || element.textContent === `${text} •`);
  assert(found && !found.disabled, `Missing enabled button: ${text}`);
  return found;
}
function nameInput(root, value) {
  const input = root.querySelector('input[placeholder="Local macro name"]');
  assert(input, "Missing name field");
  if (value !== undefined) { input.value = value; input.dispatchEvent(new Event("input", {bubbles:true})); }
  return input;
}
function fieldControl(root, label) {
  const field = [...root.querySelectorAll("label.field")].find(item => item.querySelector("span")?.textContent === label);
  assert(field, `Missing field: ${label}`); return field.querySelector("input,select");
}
function change(control, value) {
  control.value = value; control.dispatchEvent(new Event("change", {bubbles:true}));
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
  const captures = [];
  const app = mount(root, {codec, hid, storage, wait: async () => {}, prepareHost: async source => {
    const capture = {source, stopped:false, start(receive) {
      receive(source === "ScreenAverage" ? {Rgb:[24,48,96]} : {Bands:Array(32).fill(4)});
    }, async stop() { this.stopped = true; }};
    captures.push(capture); return capture;
  }});
  async function connect() {
    await app.connect();
    await until(() => !app.view.macros.discoveryBusy);
    await app.intent({type:"read", feature:"macro"});
    button(root, "Macros").click();
  }
  const notify = () => notifications.dispatchEvent(Object.assign(new Event("inputreport"), {
    device:notifications, reportId:5, data:new DataView(Uint8Array.from([6,3,0]).buffer),
  }));
  try { await connect(); await run({root, app, device, storage, connect, notify, captures}); }
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
  Object.defineProperty(input, "files", {configurable:true, value:[{size:100, text:() => pending.promise}]});
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
    assert(nameInput(h.root).value === "Old session", "Reconnect discarded the unsaved local draft");
    nameInput(h.root, "New session draft");
    pending.resolve(); await tick();
    assert(nameInput(h.root).value === "New session draft", "Old save changed the reconnected UI");
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
  ["an import retains newer edits to the same macro", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending);
    await h.app.intent({type:"edit", feature:"macro", change:{Repeat:2}});
    pending.resolve(contents); await tick();
    assert(h.app.view.macros.editor.draft.repeat_count === 2, "Late import overwrote repeat count");
    assert(nameInput(h.root).value === "", "Skipped import staged metadata");
  }],
  ["an import retains newer local name and event form edits", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending);
    nameInput(h.root, "Keep my name");
    const usage = fieldControl(h.root, "USB key usage");
    change(usage, "7");
    pending.resolve(contents); await tick();
    assert(!h.app.view.macros.editor.dirty, "Late import replaced the macro");
    assert(nameInput(h.root).value === "Keep my name" && fieldControl(h.root, "USB key usage").value === "7", "Import overwrote newer local fields");
  }],
  ["the most recently requested import wins", async h => {
    const contents = await documentFor(h.app), first = deferred(), second = deferred();
    beginImport(h.root, first); beginImport(h.root, second);
    const newest = JSON.parse(contents); newest.name = "Newest document";
    second.resolve(JSON.stringify(newest)); await until(() => h.app.view.macros.editor.dirty);
    first.resolve(contents); await tick();
    assert(nameInput(h.root).value === "Newest document", "Older import replaced the newer request");
  }],
  ["leaving Macros stops recording and releases held keys", async h => {
    await h.app.intent({type:"initializeMacro"});
    button(h.root, "Record input").click(); await until(() => h.app.view.recording);
    const down = new KeyboardEvent("keydown", {code:"KeyA", bubbles:true, cancelable:true});
    document.dispatchEvent(down);
    assert(down.defaultPrevented, "Recording did not capture the key");
    button(h.root, "Settings").click();
    assert(!h.app.view.recording, "Recording continued after navigation");
    const events = h.app.view.macros.editor.draft.events;
    assert(events.length === 2 && events[0].action.Key.pressed && !events[1].action.Key.pressed, "Held key was not released");
    const next = new KeyboardEvent("keydown", {code:"KeyB", bubbles:true, cancelable:true});
    document.dispatchEvent(next);
    assert(!next.defaultPrevented && h.app.view.macros.editor.draft.events.length === 2, "Hidden recorder captured ordinary input");
  }],
  ["disconnect retains an unsaved local name and close warns about it", async h => {
    nameInput(h.root, "Retained locally");
    await h.app.disconnect(); await h.connect();
    assert(nameInput(h.root).value === "Retained locally", "Disconnect discarded name text");
    const close = new Event("beforeunload", {cancelable:true}); window.dispatchEvent(close);
    assert(close.defaultPrevented, "Closing did not account for the local name draft");
  }],
  ["visible keymap controls save a selected binding", async h => {
    button(h.root, "Keymap").click();
    h.root.querySelector('button.key[aria-label="A"]').click();
    const select = fieldControl(h.root, "Action");
    const action = [...select.options].find(option => option.textContent === "F13");
    change(select, action.value);
    assert(h.app.view.keymap.dirty, "Action control did not stage a binding");
    button(h.root, "Save").click(); await until(() => !h.app.view.busy && !h.app.view.keymap.dirty);
    assert(h.device.calls.some(call => call.kind === "send" && call.report[0] === 0x13), "Remap did not reach the selected simulated device");
    assert(h.root.querySelector('button.key[aria-label="A · F13"]'), "Saved mapped legend missing");
  }],
  ["all 23 lighting modes remain reachable from the real selector", async h => {
    button(h.root, "Lighting").click();
    const choices = [...fieldControl(h.root, "Lighting mode").options].map(option => option.value);
    assert(choices.length === 23, "Lighting modes were lost");
    for (const value of choices) {
      change(fieldControl(h.root, "Lighting mode"), value); await tick();
      if (value.startsWith("onboard:")) {
        assert(h.app.view.lighting.editor.draft.effect === value.slice(8), `Mode ${value} did not stage`);
        await until(() => !h.app.view.busy && !h.app.view.lighting.editor.dirty);
      } else assert(h.root.querySelector('.host-card'), `Host form ${value} is missing`);
    }
  }],
  ["visible settings controls stage and save one setting", async h => {
    button(h.root, "Settings").click();
    await until(() => h.app.view.settings.editor.status === "Ready");
    const checkbox = h.root.querySelector('input[type="checkbox"]');
    const before = checkbox.checked; checkbox.checked = !before;
    checkbox.dispatchEvent(new Event("change", {bubbles:true}));
    assert(h.app.view.settings.editor.dirty, "Settings toggle did not stage");
    await until(() => !h.app.view.busy && !h.app.view.settings.editor.dirty);
    assert(h.storage.records.some(item => item.record.feature === "settings"), "Setting did not receive a backup");
  }],
  ["event form saves a paired macro and assigns its selected key", async h => {
    h.root.querySelector('button.key[aria-label="A"]').click();
    button(h.root, "Initialize empty macro").click(); await tick();
    button(h.root, "Add event").click(); await tick();
    const pressed = [...h.root.querySelectorAll('label')].find(label => label.textContent === "Press (release when unchecked)").querySelector('input');
    pressed.checked = false; pressed.dispatchEvent(new Event("change", {bubbles:true}));
    button(h.root, "Add event").click(); await tick();
    assert(h.app.view.macros.editor.draft.events.length === 2, "Event form did not add paired events");
    button(h.root, "Save and assign to selected key").click();
    await until(() => !h.app.view.busy && !h.app.view.macros.editor.dirty);
    assert(h.device.calls.some(call => call.kind === "send" && call.report[0] === 0x16), "Macro was not written");
    assert(JSON.stringify(h.app.view.keymap.draft).includes("Macro"), "Macro assignment was lost");
  }],
  ["host lighting controls restore the original bytes", async h => {
    await h.app.intent({type:"read", feature:"settings"});
    button(h.root, "Lighting").click();
    const mode = h.app.view.lighting.capabilities.host_modes.find(item => item.source === "ScreenAverage");
    const original = [...h.device.lighting];
    change(fieldControl(h.root, "Lighting mode"), `host:${mode.id}`);
    button(h.root, "Start host lighting").click(); await until(() => h.app.view.host.phase === "Active");
    button(h.root, "Stop and restore").click(); await until(() => h.app.view.host.phase === "Idle");
    assert(h.captures.length === 1 && h.captures[0].stopped, "Capture lifecycle did not stop");
    assert(JSON.stringify([...h.device.lighting]) === JSON.stringify(original), "Host stop did not restore exact onboard bytes");
    const writes = h.device.calls.filter(call => call.kind === "send" && call.report[0] < 0x80);
    const restoration = writes.at(-1)?.report;
    assert(restoration?.[0] === 0x07 && JSON.stringify(restoration.slice(1,8)) === JSON.stringify(original.slice(1,8)), "Stop did not send the onboard restoration report");
  }],
  ["diagnostic controls capture and show exact archive JSON", async h => {
    button(h.root, "Diagnostics").click(); button(h.root, "Capture archive").click();
    await until(() => h.app.view.archive.status.kind === "ready");
    button(h.root, "Review / export JSON").click(); await tick();
    const input = h.root.querySelector('textarea[aria-label="Native archive JSON"]');
    assert(input.value && JSON.parse(input.value), "Archive review contents missing");
    assert(input.closest('dialog').open, "Archive review dialog did not open");
  }],
  ["a late import retains an unfinished focused numeric field", async h => {
    const contents = await documentFor(h.app), pending = deferred();
    beginImport(h.root, pending);
    const usage = fieldControl(h.root, "USB key usage"); usage.focus(); usage.value = "19";
    usage.dispatchEvent(new Event("input", {bubbles:true}));
    pending.resolve(contents); await tick();
    assert(!h.app.view.macros.editor.dirty, "Import overwrote an unfinished form edit");
    assert(usage.isConnected && document.activeElement === usage && usage.value === "19", "Skipped import erased the unfinished control");
  }],
  ["disconnect clears held pointer capture before a new recording", async h => {
    await h.app.intent({type:"initializeMacro"});
    button(h.root, "Record input").click(); await until(() => h.app.view.recording);
    h.root.querySelector('.record-capture').dispatchEvent(new PointerEvent("pointerdown", {button:0,bubbles:true,cancelable:true}));
    await h.app.disconnect(); await h.connect();
    await h.app.intent({type:"revert",feature:"macro"});
    await h.app.intent({type:"initializeMacro"});
    button(h.root, "Record input").click(); await until(() => h.app.view.recording);
    document.dispatchEvent(new PointerEvent("pointerup", {button:0,bubbles:true}));
    assert(h.app.view.macros.editor.draft.events.length === 0, "Old pointer release leaked into new recording");
    button(h.root, "Stop recording").click();
  }],
  ["a background notification preserves a visible failure's severity", async h => {
    h.device.beforeReceive = request => { if (request[0] === 0x87) throw new Error("Read failed"); };
    await h.app.intent({type:"read", feature:"lighting"});
    const status = h.root.querySelector('.status'), message = status.textContent;
    assert(status.classList.contains('error'), "Read failure was not marked");
    h.notify();
    assert(status.textContent === message && status.classList.contains('error'), "Notification erased failure severity");
  }],
  ["closing or cancelling the discard dialog settles its pending action", async h => {
    const discard = createDiscardDialog({doc:document, win:window});
    h.root.append(discard.dialog);
    try {
      let accepted;
      const closed = discard.confirm().then(result => { accepted = result; });
      discard.dialog.close(); await until(() => accepted !== undefined); await closed;
      assert(accepted === false, "Closing left the pending discard undecided");
      const cancelled = discard.confirm();
      discard.dialog.dispatchEvent(new Event("cancel", {cancelable:true}));
      assert(await cancelled === false, "Escape/cancel did not decline discard");
      discard.dialog.close(); await tick();
      const confirmed = discard.confirm(); button(h.root, "Discard changes").click();
      assert(await confirmed === true, "Explicit discard did not accept");
    } finally { discard.destroy(); discard.dialog.remove(); }
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
