// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { Reader, selectDevice, validateDevice, payloadFromView } from "./transport.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

function device() {
  const calls = [];
  let opcode;
  return {
    vendorId: 0x3151, productId: 0x4011, opened: false, calls,
    collections: [{ usagePage: 0xffff, usage: 2, children: [],
      featureReports: [{ reportId: 0, items: [{ reportSize: 8, reportCount: 64 }] }] }],
    async open() { calls.push("open"); this.opened = true; },
    async close() { calls.push("close"); this.opened = false; },
    async sendFeatureReport(id, payload) {
      calls.push(["send", id, [...payload]]);
      opcode = payload[0];
    },
    async receiveFeatureReport(id) {
      calls.push(["receive", id]);
      // Nonzero DataView offset proves we don't consume the whole backing buffer.
      const bytes = new Uint8Array(70);
      bytes.set([opcode, 1, 0, 4, 7, 250, 255, 250], 3);
      bytes[66] = 0xa5;
      return new DataView(bytes.buffer, 3, 64);
    },
  };
}

const immediate = async () => {};

test("rejects wrong product, collection, numbered, short and duplicate reports before opening", () => {
  const mutations = [
    d => d.vendorId = 1,
    d => d.productId = 1,
    d => d.collections[0].usage = 1,
    d => d.collections.push(d.collections[0]),
    d => d.collections[0].featureReports[0].reportId = 1,
    d => d.collections[0].featureReports[0].items[0].reportCount = 63,
    d => d.collections[0].children.push({ children: [], featureReports: d.collections[0].featureReports }),
  ];
  for (const mutate of mutations) {
    const selected = device();
    mutate(selected);
    assert.throws(() => new Reader(selected, codec));
    assert.deepEqual(selected.calls, []);
  }
  assert.throws(() => selectDevice([]), { code: "selection" });
  assert.throws(() => selectDevice([device(), device()]), { code: "selection" });
  const alternate = device();
  alternate.productId = 0x4015;
  assert.equal(validateDevice(alternate), alternate);
});

test("real WASM codec sends only the three exact read headers, paces and preserves raw white", async () => {
  const selected = device();
  const reader = new Reader(selected, codec, async ms => selected.calls.push(["wait", ms]));
  const snapshot = await reader.readAll();
  assert.deepEqual(snapshot.lighting.setting.rgb, [255, 255, 255]);
  assert.deepEqual(snapshot.lighting.raw.slice(5, 8), [250, 255, 250]);
  assert.equal(snapshot.lighting.raw[63], 0xa5);
  assert.deepEqual(selected.calls.slice(1).map(call => call[0]),
    ["send", "wait", "receive", "send", "wait", "receive", "send", "wait", "receive"]);
  const sends = selected.calls.filter(call => call[0] === "send");
  assert.deepEqual(sends.map(call => [call[1], call[2][0], call[2][7], call[2].length]),
    [[0, 0x80, 0x7f, 64], [0, 0x85, 0x7a, 64], [0, 0x87, 0x78, 64]]);
  assert(selected.calls.filter(call => call[0] === "wait").every(call => call[1] === 30));
  const count = selected.calls.length;
  await reader.readAll();
  assert.equal(selected.calls.length, count, "navigation/cache access causes no reads");
  await reader.readAll(true);
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 6);
  await reader.close();
});

test("concurrent requests serialize, sharing accepted observations", async () => {
  const selected = device();
  const reader = new Reader(selected, codec, immediate);
  const [first, second] = await Promise.all([reader.readAll(), reader.readAll()]);
  assert.deepEqual(first, second);
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 3);
});

test("a failed read is not retried automatically, next explicit read can succeed", async () => {
  const selected = device();
  const receive = selected.receiveFeatureReport;
  selected.receiveFeatureReport = async () => { throw new Error("USB read failed"); };
  const reader = new Reader(selected, codec, immediate);
  await assert.rejects(reader.readAll(), /USB read failed/);
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 1);
  selected.receiveFeatureReport = receive;
  await reader.readAll();
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 4);
});

test("a failed initial sweep does not mix partial cached replies into the retry", async () => {
  const selected = device();
  const receive = selected.receiveFeatureReport;
  let receives = 0;
  selected.receiveFeatureReport = async function(id) {
    if (++receives === 2) throw new Error("profile failed");
    return receive.call(this, id);
  };
  const reader = new Reader(selected, codec, immediate);
  await assert.rejects(reader.readAll(), /profile failed/);
  await reader.readAll();
  assert.deepEqual(selected.calls.filter(call => call[0] === "send").map(call => call[2][0]),
    [0x80, 0x85, 0x80, 0x85, 0x87]);
});

test("the captured physical WebHID replies decode through the same WASM boundary", async () => {
  const capture = JSON.parse(await readFile(new URL("../Research/webhid-read-20260929.json", import.meta.url)));
  for (const name of ["identity", "profile", "lighting"]) {
    assert.deepEqual(JSON.parse(codec.decode_reply(name, Uint8Array.from(capture[name].raw))), capture[name]);
  }
});

test("disconnect rejects a late response and closes only the selected object", async () => {
  const selected = device();
  let release, entered;
  const receiving = new Promise(resolve => { entered = resolve; });
  selected.receiveFeatureReport = () => {
    entered();
    return new Promise(resolve => { release = resolve; });
  };
  const reader = new Reader(selected, codec, immediate);
  const reading = reader.readAll();
  await receiving;
  const closing = reader.close();
  release(new DataView(new Uint8Array(64).buffer));
  await assert.rejects(reading, { code: "disconnected" });
  await closing;
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 1);
  assert.equal(selected.calls.at(-1), "close");
  await assert.rejects(reader.readAll(), { code: "disconnected" });
});

test("disconnect during pacing prevents the pending receive", async () => {
  const selected = device();
  let reader;
  let closing;
  reader = new Reader(selected, codec, async () => { closing = reader.close(); });
  await assert.rejects(reader.readAll(), { code: "disconnected" });
  await closing;
  assert.equal(selected.calls.filter(call => call[0] === "receive").length, 0);
});

test("WebHID framing rejects native ID-prefix buffers and the codec rejects wrong replies", async () => {
  for (const size of [0, 63, 65]) {
    assert.throws(() => payloadFromView(new DataView(new ArrayBuffer(size))), { code: "response-shape" });
  }
  const selected = device();
  selected.receiveFeatureReport = async () => new DataView(new Uint8Array(64).buffer);
  const reader = new Reader(selected, codec, immediate);
  await assert.rejects(reader.readAll());
  assert.equal(selected.calls.filter(call => call[0] === "send").length, 1);
});
