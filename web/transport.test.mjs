// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { filters, selectDevice, validateDevice, payloadFromView } from "./transport.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

function device() {
  return {
    vendorId: 0x3151, productId: 0x4011,
    collections: [{ usagePage: 0xffff, usage: 2, children: [],
      featureReports: [{ reportId: 0, items: [{ reportSize: 8, reportCount: 64 }] }] }],
  };
}

test("the selected Nia87 vendor collection has one unnumbered 64-byte feature report", () => {
  assert.deepEqual(filters.map(({ vendorId, productId, usagePage, usage }) =>
    [vendorId, productId, usagePage, usage]),
    [[0x3151, 0x4011, 0xffff, 2], [0x3151, 0x4015, 0xffff, 2]]);
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
    const candidate = device();
    mutate(candidate);
    assert.throws(() => validateDevice(candidate));
  }
  assert.throws(() => selectDevice([]), { code: "selection" });
  assert.throws(() => selectDevice([device(), device()]), { code: "selection" });
  const alternate = device();
  alternate.productId = 0x4015;
  assert.equal(selectDevice([alternate]), alternate);
});

test("the Rust codec emits only the three supported exact read headers", () => {
  for (const [name, opcode, checksum] of [
    ["identity", 0x80, 0x7f], ["profile", 0x85, 0x7a], ["lighting", 0x87, 0x78],
  ]) {
    const request = [...codec.read_request(name)];
    assert.equal(request.length, 64);
    assert.deepEqual(request.slice(0, 8), [opcode, 0, 0, 0, 0, 0, 0, checksum]);
    assert(request.slice(8).every(byte => byte === 0));
  }
  assert.throws(() => codec.read_request("write"));
});

test("WebHID DataView offsets preserve raw reply bytes and semantic white", () => {
  const backing = new Uint8Array(70);
  backing.set([0x87, 1, 0, 4, 7, 250, 255, 250], 3);
  backing[66] = 0xa5;
  const payload = payloadFromView(new DataView(backing.buffer, 3, 64));
  assert.equal(payload.length, 64);
  assert.equal(payload[63], 0xa5);
  const lighting = JSON.parse(codec.decode_reply("lighting", payload));
  assert.deepEqual(lighting.setting.rgb, [255, 255, 255]);
  assert.deepEqual(lighting.raw.slice(5, 8), [250, 255, 250]);
  assert.equal(lighting.raw[63], 0xa5);
  payload[0] = 0; // The browser result is a copy of the DataView slice.
  assert.equal(backing[3], 0x87);
});

test("synthetic Nia87 replies decode through the WASM boundary", () => {
  for (const [name, header] of [
    ["identity", [0x80, 0, 1]],
    ["profile", [0x85]],
    ["lighting", [0x87, 1, 4, 4, 7, 12, 34, 56]],
  ]) {
    const raw = new Uint8Array(64);
    raw.set(header);
    const decoded = JSON.parse(codec.decode_reply(name, raw));
    assert.deepEqual(decoded.raw, [...raw]);
    if (name === "lighting") {
      assert.equal(decoded.effectName, "LightAlwaysOn");
      assert.equal(decoded.setting.value, 4);
      assert.deepEqual(decoded.setting.rgb, [12, 34, 56]);
    }
  }
});

test("WebHID framing and the codec reject malformed or unrelated replies", () => {
  for (const size of [0, 63, 65]) {
    assert.throws(() => payloadFromView(new DataView(new ArrayBuffer(size))), { code: "response-shape" });
  }
  assert.throws(() => codec.decode_reply("lighting", new Uint8Array(64)));
  assert.throws(() => codec.decode_reply("identity", Uint8Array.from([0x80])));
});
