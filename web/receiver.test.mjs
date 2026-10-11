// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { DeviceExecutor } from "./executor.mjs";
import { createRuntime } from "./runtime.mjs";
import { TestDevice, TestStore } from "./test-device.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

class ReceiverDevice extends TestDevice {
  productId = 0x4011;
  productName = "Simulated Nia87 receiver";
  keyboard = new TestDevice();
  selected = false;
  pending = false;
  online = true;
  ready = true;
  boardId = 0x095f;
  statusFlag = null;
  showReply = true;
  reply = null;
  constructor() {
    super();
    this.keyboard.opened = true;
  }
  async sendFeatureReport(id, data) {
    if (id !== 0 || data.length !== 64 || !this.opened) throw Error("Unexpected receiver report");
    const report = Uint8Array.from(data);
    this.calls.push({ kind: "send", report: [...report] });
    switch (report[0]) {
      case 0xf7: {
        const status = new Uint8Array(64);
        status[0] = Number(this.pending && this.showReply);
        status[3] = Number(!this.online);
        status[5] = Number(this.ready);
        status[7] = 0x7f; // Opaque retained bytes are not status flags.
        if (this.statusFlag !== null) status[5] = this.statusFlag;
        this.reply = status;
        return;
      }
      case 0xf6:
        assert.equal(report[1], 10);
        this.selected = true;
        return;
      case 0xfc:
        if (!this.pending) throw Error("No relayed reply to release");
        this.reply = this.keyboardReply;
        this.pending = false;
        return;
      default:
        if (!this.selected) throw Error("Keyboard report sent without receiver target selection");
        this.selected = false;
        if (report[0] === 0x8f) {
          this.keyboardReply = new Uint8Array(64);
          this.keyboardReply.set([0x8f, this.boardId & 0xff, this.boardId >> 8]);
          this.pending = true;
        } else {
          await this.keyboard.sendFeatureReport(0, report);
          if (report[0] >= 0x80) {
            const view = await this.keyboard.receiveFeatureReport(0);
            this.keyboardReply = new Uint8Array(view.buffer, view.byteOffset, view.byteLength).slice();
            this.pending = true;
          }
        }
    }
  }
  async receiveFeatureReport(id) {
    if (id !== 0 || !this.opened || !this.reply) throw Error("Unexpected receiver GET");
    this.calls.push({ kind: "receive" });
    const result = this.reply;
    this.reply = null;
    return new DataView(result.buffer.slice(0));
  }
}

function harness() {
  const device = new ReceiverDevice();
  const store = new TestStore(device.calls);
  const executor = new DeviceExecutor(device, codec.BrowserOperation, store,
    async ms => device.calls.push({ kind: "wait", ms }), codec);
  const session = new codec.BrowserSession();
  async function send(input) {
    let result = JSON.parse(session.dispatch(JSON.stringify(input)));
    while (result.command) result = JSON.parse(session.accept(JSON.stringify(await executor.run(result.command))));
    return result;
  }
  async function close() { await executor.close(); session.free(); }
  return { device, store, executor, session, send, close };
}

const sends = device => device.calls.filter(call => call.kind === "send").map(call => call.report[0]);

test("receiver identity and feature reads use one bounded selected-keyboard relay exchange", async () => {
  const h = harness();
  try {
    await h.executor.verifyReceiver();
    assert.deepEqual(sends(h.device), [0xf7, 0xf6, 0x8f, 0xf7, 0xfc]);
    h.device.calls.length = 0;
    await h.send({ type: "connect" });
    const result = await h.send({ type: "read", feature: "lighting" });
    assert.equal(result.outcome.kind, "lightingLoaded");
    assert.deepEqual(sends(h.device), [0xf7, 0xf6, 0x87, 0xf7, 0xfc]);
    assert.deepEqual(h.device.calls.filter(call => call.kind === "wait").map(call => call.ms), [10, 30, 100, 10, 10]);
    assert.equal(h.device.calls.filter(call => call.kind === "receive").length, 3);
  } finally { await h.close(); }
});

test("receiver keymap read returns raw pages only after each fresh relay reply", async () => {
  const h = harness();
  try {
    await h.executor.verifyReceiver();
    await h.send({ type: "connect" });
    h.device.calls.length = 0;
    const result = await h.send({ type: "read", feature: "keymap" });
    assert.equal(result.outcome.kind, "loaded");
    const opcodes = sends(h.device);
    assert.equal(opcodes.filter(opcode => opcode === 0x89).length, 8);
    assert.equal(opcodes.filter(opcode => opcode === 0x90).length, 8);
    assert.equal(opcodes.filter(opcode => opcode === 0xfc).length, 18);
    assert.equal(h.device.calls.filter(call => call.kind === "receive").length, 54);
    assert.deepEqual(JSON.parse(h.session.view()).keymap.baseline.bindings.base["slot-009"], { Key: 4 });
  } finally { await h.close(); }
});

test("pending, sleeping and malformed receiver status reject before keyboard command", async () => {
  for (const [change, diagnostic] of [
    [device => { device.pending = true; }, /unread reply/],
    [device => { device.online = false; }, /asleep or disconnected/],
    [device => { device.statusFlag = 2; }, /Invalid receiver status flag/],
  ]) {
    const h = harness();
    try {
      change(h.device);
      await assert.rejects(h.executor.verifyReceiver(), diagnostic);
      assert.deepEqual(sends(h.device), [0xf7]);
    } finally { await h.close(); }
  }
});

test("receiver polling remains bounded when command or reply never becomes ready", async () => {
  const command = harness();
  try {
    command.device.ready = false;
    await assert.rejects(command.executor.verifyReceiver(), /did not become ready/);
    assert.deepEqual(sends(command.device), Array(5).fill(0xf7));
    assert.equal(command.device.calls.filter(call => call.kind === "wait" && call.ms === 100).length, 4);
  } finally { await command.close(); }

  const reply = harness();
  try {
    reply.device.showReply = false;
    await assert.rejects(reply.executor.verifyReceiver(), /did not return a reply/);
    assert.deepEqual(sends(reply.device), [0xf7, 0xf6, 0x8f, ...Array(5).fill(0xf7)]);
    assert.equal(reply.device.calls.filter(call => call.kind === "wait" && call.ms === 100).length, 5);
  } finally { await reply.close(); }
});

test("wrong board identity fails closed before feature loading", async () => {
  const h = harness();
  try {
    h.device.boardId = 0x0960;
    await assert.rejects(h.executor.verifyReceiver(), /unsupported keyboard/);
    assert.deepEqual(sends(h.device), [0xf7, 0xf6, 0x8f, 0xf7, 0xfc]);
  } finally { await h.close(); }
});

test("receiver lighting save keeps backup and pacing, with no reply fetch after SET", async () => {
  const h = harness();
  try {
    await h.executor.verifyReceiver();
    await h.send({ type: "connect" });
    await h.send({ type: "read", feature: "lighting" });
    await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [1, 2, 3] } } });
    h.device.calls.length = 0;
    const result = await h.send({ type: "apply", feature: "lighting" });
    assert.equal(result.outcome.kind, "lightingSaved");
    assert.equal(h.device.calls[0].kind, "backup");
    assert.deepEqual(sends(h.device), [0xf7, 0xf6, 0x07]);
    assert.equal(h.device.calls.filter(call => call.kind === "receive").length, 1);
    assert.deepEqual(h.device.calls.filter(call => call.kind === "wait").map(call => call.ms), [10, 500]);
    assert.deepEqual([...h.device.keyboard.lighting.slice(5, 8)], [1, 2, 3]);
  } finally { await h.close(); }
});

test("receiver status failure before a setter retains the draft without attempting the setter", async () => {
  const h = harness();
  try {
    await h.executor.verifyReceiver();
    await h.send({ type: "connect" });
    await h.send({ type: "read", feature: "lighting" });
    await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [1, 2, 3] } } });
    h.device.calls.length = 0;
    h.device.pending = true;
    const result = await h.send({ type: "apply", feature: "lighting" });
    assert.equal(result.outcome.kind, "failed");
    assert.equal(h.device.calls[0].kind, "backup");
    assert.deepEqual(sends(h.device), [0xf7]);
    assert.equal(JSON.parse(h.session.view()).lighting.editor.dirty, true);
  } finally { await h.close(); }
});

test("disconnect after receiver selection does not report an unsubmitted setter", async () => {
  const h = harness();
  try {
    await h.executor.verifyReceiver();
    await h.send({ type: "connect" });
    await h.send({ type: "read", feature: "lighting" });
    await h.send({ type: "edit", feature: "lighting", change: { Color: { Rgb: [1, 2, 3] } } });
    h.device.calls.length = 0;

    const originalSend = h.device.sendFeatureReport.bind(h.device);
    let disconnected;
    h.device.sendFeatureReport = (id, data) => {
      const sent = originalSend(id, data);
      if (data[0] !== 0xf6) return sent;
      return {
        then(resolve, reject) {
          sent.then(() => {
            resolve();
            // The F6 send has completed; retire the executor before the write resumes.
            queueMicrotask(() => { disconnected = h.executor.close(); });
          }, reject);
        },
      };
    };

    const result = await h.send({ type: "apply", feature: "lighting" });
    await disconnected;
    assert.equal(result.outcome.kind, "failed");
    assert.equal(h.device.calls[0].kind, "backup");
    assert.deepEqual(sends(h.device), [0xf7, 0xf6]);
    assert.equal(h.device.calls.filter(call => call.kind === "wait" && call.ms === 1000).length, 0);
    const editor = JSON.parse(h.session.view()).lighting.editor;
    assert.equal(editor.status.Unverified.problem.Apply.recovery, "NotAttempted");
    assert.equal(editor.dirty, true);
  } finally { await h.close(); }
});

test("failed receiver identity leaves the browser disconnected with a useful status", async () => {
  for (const [change, diagnostic] of [
    [device => { device.online = false; }, /Press a key/],
    [device => { device.boardId = 0x0960; }, /unsupported keyboard/],
  ]) {
    const device = new ReceiverDevice(), hid = new EventTarget();
    change(device);
    hid.requestDevice = async () => [device];
    const runtime = createRuntime({ codec, hid, storage: new TestStore(), now: () => 0,
      wait: async () => {}, prepareHost: async () => { throw Error("capture started"); },
      win: globalThis, changed: () => {}, isEditing: () => false,
      connected: () => assert.fail("unsupported receiver connected"), disconnected: () => {},
    });
    try {
      await runtime.connect();
      assert.equal(runtime.connection.device, null);
      assert.equal(runtime.notice.error, true);
      assert.match(runtime.notice.message, diagnostic);
      assert.equal(sends(device).filter(opcode => opcode === 0x80).length, 0);
    } finally { await runtime.destroy(); }
  }
});

test("receiver host work is rejected before capture preparation or USB frames", async () => {
  const device = new ReceiverDevice(), storage = new TestStore(), hid = new EventTarget();
  hid.requestDevice = async () => [device];
  let prepared = 0;
  const runtime = createRuntime({ codec, hid, storage, now: () => 0,
    wait: async () => {}, prepareHost: async () => { prepared++; throw Error("capture started"); },
    win: globalThis, changed: () => {}, isEditing: () => false,
    connected: () => {}, disconnected: () => {},
  });
  try {
    await runtime.connect();
    assert.equal(runtime.hostModesAvailable, false);
    assert.match(runtime.connection.notificationStatus, /unverified/);
    const mode = runtime.view.lighting.capabilities.host_modes[0];
    await assert.rejects(runtime.startHost(mode, null), /not verified/);
    assert.equal((await runtime.intent({ type: "hostStart", mode: mode.id, setting: null })).ok, false);
    assert.equal(prepared, 0);
    const freed = { value: false };
    const executor = new DeviceExecutor(device, codec.BrowserOperation, storage, async () => {}, codec);
    await assert.rejects(executor.runHost({ free() { freed.value = true; } }), /not verified/);
    assert.equal(freed.value, true);
    await executor.close();
  } finally { await runtime.destroy(); }
});
