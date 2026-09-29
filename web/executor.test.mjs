// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { DeviceExecutor } from "./executor.mjs";
import { TestDevice, TestStore } from "./test-device.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

function harness() {
  const device = new TestDevice();
  const store = new TestStore(device.calls);
  const executor = new DeviceExecutor(device, codec.BrowserOperation, store, async ms => device.calls.push({ kind: "wait", ms }));
  const session = new codec.BrowserSession();
  async function send(intent) {
    let result = JSON.parse(session.dispatch(JSON.stringify(intent)));
    while (result.command) result = JSON.parse(session.accept(JSON.stringify(await executor.run(result.command))));
    return result;
  }
  const view = () => JSON.parse(session.view());
  return { device, store, executor, session, send, view };
}
const writes = device => device.calls.filter(call => call.kind === "send" && call.report[0] < 0x80);
const reads = device => device.calls.filter(call => call.kind === "receive");

test("core key remap backs up first, preserves protected Fn, sends one change and verifies once", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "keymap" });
  const rejected = await h.send({ type: "edit", feature: "keymap", change: { layer: "fn", key: "slot-000", action: { Key: 5 } } });
  assert.equal(rejected.ok, false);
  const originalFn = h.device.fn.slice();
  await h.send({ type: "edit", feature: "keymap", change: { layer: "base", key: "slot-009", action: { Key: 5 } } });
  h.device.calls.length = 0;
  const result = await h.send({ type: "apply", feature: "keymap" });
  assert.equal(result.outcome.kind, "saved");
  assert.equal(h.device.calls[0].kind, "backup");
  assert.equal(writes(h.device).length, 1);
  assert.deepEqual(writes(h.device)[0].report.slice(8, 12), [0, 0, 5, 0]);
  assert.equal(reads(h.device).length, 18);
  assert(h.device.calls.some(call => call.kind === "wait" && call.ms === 1000));
  assert.deepEqual(h.device.fn, originalFn);
  assert.equal(h.view().keymap.dirty, false);
});

test("global RGB uses shared white convention and transport acceptance without a getter", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "lighting" });
  await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [255, 255, 255] } } });
  h.device.calls.length = 0;
  const result = await h.send({ type: "apply", feature: "lighting" });
  assert.equal(result.outcome.kind, "lightingSaved");
  assert.equal(h.device.calls[0].kind, "backup");
  assert.deepEqual(writes(h.device)[0].report.slice(5, 8), [250, 255, 250]);
  assert.equal(reads(h.device).length, 0);
  assert.equal(h.view().lighting.editor.baseline.evidence, "TransportAccepted");
  assert.equal(h.view().lighting.editor.baseline.revision[63], 0xa5);
  assert(h.device.calls.some(call => call.kind === "wait" && call.ms === 500));
});

test("per-key layer workflow loads selected colors then uploads without getter or hidden-slot loss", async () => {
  const h = harness();
  h.device.lighting.set([0x87, 13, 4, 4, 0x10, 0, 200, 200]);
  const other = h.device.pictures[0].slice();
  const padding = h.device.pictures[1].slice(378);
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "lighting" });
  await h.send({ type: "preparePicture" });
  await h.send({ type: "edit", feature: "picture", change: { Color: { key: "slot-009", color: [7, 8, 9] } } });
  const selector = await h.send({ type: "edit", feature: "lighting", change: { Option: "3" } });
  assert.equal(selector.ok, false, "dirty colors block selector changes");
  h.device.calls.length = 0;
  const result = await h.send({ type: "apply", feature: "picture" });
  assert.equal(result.outcome.kind, "pictureSaved");
  assert.equal(h.device.calls[0].kind, "backup");
  assert.equal(writes(h.device).length, 7);
  assert.equal(reads(h.device).length, 0);
  assert.equal(h.device.calls.filter(call => call.kind === "wait" && call.ms === 20).length, 7);
  assert.deepEqual(h.device.pictures[1].slice(27, 30), new Uint8Array([7, 8, 9]));
  assert.deepEqual(h.device.pictures[1].slice(378), padding);
  assert.deepEqual(h.device.pictures[0], other);
});

test("slot49 macro save and assign uses five safe pages, one readback, and core continuation", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "keymap" });
  await h.send({ type: "selectMacro", slot: "slot-49" });
  await h.send({ type: "read", feature: "macro" });
  await h.send({ type: "initializeMacro" });
  for (const [at, pressed] of [true, false].entries()) {
    await h.send({ type: "edit", feature: "macro", change: { Insert: { at, event: { action: { Key: { usage: 4, pressed } }, delay_ms: 10 } } } });
  }
  h.device.calls.length = 0;
  const result = await h.send({ type: "saveAndAssignMacro", layer: "base", key: "slot-009", binding: "counted" });
  assert.equal(result.outcome.kind, "assignmentSucceeded");
  const macroWrites = writes(h.device).filter(call => call.report[0] === 0x16);
  assert.equal(macroWrites.length, 5);
  assert.equal(macroWrites[4].report[3], 26);
  assert.equal(reads(h.device).filter(call => call.opcode === 0x8b).length, 4);
  assert(h.device.calls.some(call => call.kind === "wait" && call.ms === 2030));
  assert.deepEqual([...h.device.base.slice(36, 40)], [9, 0, 49, 0]);
  assert.equal(h.store.records.length, 2);
});

test("rejected backup prevents every setter and retains the staged RGB", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "lighting" });
  await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [1, 2, 3] } } });
  h.store.save = async () => { throw new Error("IndexedDB quota exceeded"); };
  const result = await h.send({ type: "apply", feature: "lighting" });
  assert.equal(result.outcome.kind, "failed");
  assert.equal(writes(h.device).length, 0);
  assert.equal(h.view().lighting.editor.dirty, true);
  assert.equal(h.view().lighting.editor.status.Unverified.problem.Apply.recovery, "NotAttempted");
});

test("first open failure performs no recovery writes", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "keymap" });
  await h.send({ type: "edit", feature: "keymap", change: { layer: "base", key: "slot-009", action: { Key: 5 } } });
  h.device.opened = false;
  h.device.open = async () => { throw new Error("Keyboard inaccessible"); };
  await h.send({ type: "apply", feature: "keymap" });
  assert.equal(writes(h.device).length, 0);
  assert.equal(h.view().keymap.status.Unverified.problem.Apply.recovery, "NotAttempted");
});

test("failed macro request recovers and verifies the exact before-image, retaining draft", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "macro" });
  await h.send({ type: "initializeMacro" });
  let failed = false;
  h.device.beforeSend = report => {
    if (report[0] === 0x16 && report[2] === 1 && !failed) { failed = true; throw new Error("Synthetic transport failure"); }
  };
  await h.send({ type: "apply", feature: "macro" });
  assert.equal(h.view().macros.editor.status.Unverified.problem.Apply.recovery, "Verified");
  assert.equal(h.view().macros.editor.dirty, true);
  assert.deepEqual(h.device.macros[0], new Uint8Array(256));
  assert(h.device.calls.some(call => call.kind === "wait" && call.ms === 1000));
});

test("foreground lighting read runs between passive catalog slots", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  let foreground;
  h.device.beforeReceive = request => {
    if (request[0] === 0x8b && request[1] === 0 && request[2] === 1) {
      foreground = h.send({ type: "read", feature: "lighting" });
    }
  };
  await h.send({ type: "discoverMacros" });
  await foreground;
  const calls = reads(h.device);
  const lightingIndex = calls.findIndex(call => call.opcode === 0x87);
  assert.equal(lightingIndex, 4);
  assert.equal(calls[lightingIndex + 1].slot, 1);
  assert.equal(h.view().macros.occupancy["slot-49"], "Empty");
});

test("a later RGB edit remains staged when the submitted color completes", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  await h.send({ type: "read", feature: "lighting" });
  await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [1, 2, 3] } } });
  h.device.beforeSend = async report => {
    if (report[0] === 7) {
      h.device.beforeSend = null;
      const edited = await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [4, 5, 6] } } });
      assert.equal(edited.ok, true);
    }
  };
  await h.send({ type: "apply", feature: "lighting" });
  assert.deepEqual(h.view().lighting.editor.baseline.content.Editable.color, { Rgb: [1, 2, 3] });
  assert.deepEqual(h.view().lighting.editor.draft.color, { Rgb: [4, 5, 6] });
  assert.equal(h.view().lighting.editor.dirty, true);
  await h.send({ type: "apply", feature: "lighting" });
  assert.equal(h.view().lighting.editor.dirty, false);
});

test("disconnect rejects a delayed feature completion without reopening the selected device", async () => {
  const h = harness();
  await h.send({ type: "connect" });
  let reachedReceive, releaseReceive;
  const reached = new Promise(resolve => { reachedReceive = resolve; });
  const release = new Promise(resolve => { releaseReceive = resolve; });
  h.device.beforeReceive = async () => { reachedReceive(); await release; };
  const reading = h.send({ type: "read", feature: "lighting" });
  await reached;
  await h.send({ type: "disconnect" });
  const closing = h.executor.close();
  releaseReceive();
  const result = await reading;
  await closing;
  assert.equal(result.outcome.kind, "ignored");
  assert.equal(h.view().connection.kind, "disconnected");
  assert.equal(h.view().lighting.editor.baseline, null);
  assert.equal(h.device.calls.filter(call => call.kind === "open").length, 1);
  assert.equal(h.device.calls.at(-1).kind, "close");
});
