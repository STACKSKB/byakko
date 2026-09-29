// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { DeviceExecutor } from "./executor.mjs";
import { TestDevice, TestStore } from "./test-device.mjs";
await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

function harness() {
  const device = new TestDevice(), session = new codec.BrowserSession();
  const executor = new DeviceExecutor(device, codec.BrowserOperation, new TestStore(), async () => {});
  const dispatch = value => JSON.parse(session.dispatch(JSON.stringify(value)));
  const send = async value => {
    let result = dispatch(value);
    while (result.command) result = JSON.parse(session.accept(JSON.stringify(await executor.run(result.command))));
    return result;
  };
  const view = () => JSON.parse(session.view());
  const notify = (payload, nowMs, generation = view().connection.generation) => dispatch({type:"notification",generation,reportId:5,payload,nowMs});
  return {device, session, executor, dispatch, send, view, notify};
}

test("captured physical notifications coalesce to affected loaded lighting only", async () => {
  const h = harness();
  await h.send({type:"connect"});
  await h.send({type:"read",feature:"keymap"});
  await h.send({type:"read",feature:"lighting"});
  const capture = JSON.parse(await readFile(new URL('../Research/webhid-notifications-20260930.json',import.meta.url)));
  let at = 1000;
  for (const event of capture.reports) h.notify(event.payload, at += 10);
  const due = h.view().observation.dueMs;
  h.device.calls.length = 0;
  assert.equal(h.dispatch({type:"observeNext",nowMs:due-1}).command, null);
  h.device.lighting[3] = 2;
  await h.send({type:"observeNext",nowMs:due});
  await h.send({type:"observeNext",nowMs:due+1});
  assert.deepEqual(h.device.calls.filter(x=>x.kind==='receive').map(x=>x.opcode),[0x87]);
  assert.equal(h.view().lighting.editor.draft.brightness, 2);
  assert.equal(h.view().observation.queued,false);
  assert(!h.device.calls.some(x=>x.kind==='send'&&x.report[0]<0x80));
  await h.executor.close(); h.session.free();
});

test("late notifications are ignored and edits made during refresh become retained conflicts", async () => {
  const h = harness();
  await h.send({type:"connect"});
  await h.send({type:"read",feature:"lighting"});
  const generation = h.view().connection.generation;
  h.notify([4,1,0],0,generation-1);
  assert.equal(h.view().observation.queued,false);
  h.notify([4,1,0],0);
  const read = h.dispatch({type:"observeNext",nowMs:500});
  assert.equal(h.view().blocksEditing,false);
  h.dispatch({type:"edit",feature:"lighting",change:{Brightness:1}});
  h.device.lighting[3] = 2;
  JSON.parse(h.session.accept(JSON.stringify(await h.executor.run(read.command))));
  assert.equal(h.view().lighting.editor.draft.brightness,1);
  assert(h.view().lighting.editor.status.Conflict);
  assert.equal(h.view().observation.canRead,false);
  await h.executor.close(); h.session.free();
});

test("settings notifications read each section once and configuration waits two seconds", async () => {
  const h = harness();
  await h.send({type:"connect"});
  await h.send({type:"read",feature:"settings"});
  h.device.calls.length = 0;
  h.notify([3,1,9],100);
  await h.send({type:"observeNext",nowMs:600});
  assert.deepEqual(h.device.calls.filter(x=>x.kind==='receive').map(x=>x.opcode),[0x91,0x97,0x92,0x86]);
  h.device.calls.length = 0;
  h.notify([13,0,0],1000);
  assert.equal(h.dispatch({type:"observeNext",nowMs:2999}).command,null);
  await h.send({type:"observeNext",nowMs:3000});
  assert.equal(h.device.calls.filter(x=>x.kind==='receive').length,4);
  await h.executor.close(); h.session.free();
});
