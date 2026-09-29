// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { DeviceExecutor } from "./executor.mjs";
import { HostController } from "./host.mjs";
import { TestDevice, TestStore } from "./test-device.mjs";
await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm",import.meta.url)) });
const deferred = () => { let resolve, reject; const promise = new Promise((r,j)=>{resolve=r;reject=j;}); return {promise,resolve,reject}; };
async function harness(prepareOverride, dispatchOverride) {
  const device = new TestDevice(), session = new codec.BrowserSession();
  const store = new TestStore(device.calls);
  const executor = new DeviceExecutor(device,codec.BrowserOperation,store,async ms=>device.calls.push({kind:"wait",ms}));
  const view = () => JSON.parse(session.view());
  const dispatch = async value => {
    let result = JSON.parse(session.dispatch(JSON.stringify(value)));
    while (result.command) result = JSON.parse(session.accept(JSON.stringify(await executor.run(result.command))));
    return result;
  };
  await dispatch({type:"connect"});
  await dispatch({type:"read",feature:"lighting"});
  await dispatch({type:"read",feature:"settings"});
  const original = [...device.lighting];
  let frame, failed, stopped = 0;
  const prepare = prepareOverride ?? (async () => ({label:"Synthetic capture",start(receive,error){frame=receive;failed=error;},async stop(){stopped++;}}));
  const host = new HostController({codec,dispatch: value => dispatchOverride ? dispatchOverride(dispatch,value) : dispatch(value),executor,prepare});
  return {device,session,executor,host,view,dispatch,original,frame:value=>frame(value),fail:error=>failed(error),stopped:()=>stopped,
    mode:id=>view().lighting.capabilities.host_modes.find(mode=>mode.id===id)};
}
test("host start/update/stop preserves original bytes and shares durable serialized effects", async () => {
  const h = await harness();
  const mode = h.mode("music-follow-2");
  h.device.calls.length = 0;
  await h.host.start(mode,mode.parameters.default);
  assert.equal(h.view().host.phase,"Active");
  assert.equal(h.device.calls[0].kind,"backup");
  const parameters = {...mode.parameters.default,brightness:2};
  await h.host.update(parameters);
  h.frame({Bands:Array(32).fill(4)});
  await h.host.stop();
  assert.equal(h.view().host.phase,"Idle");
  assert.equal(h.host.state.problem,null);
  assert.deepEqual([...h.device.lighting],h.original);
  assert.equal(h.device.calls.filter(x=>x.kind==='receive').length,2); // start and stop only
  assert.equal(h.view().lighting.editor.status,"Ready");
  await h.executor.close(); h.session.free();
});
test("stop during source preparation performs no keyboard writes and closes late capture", async () => {
  const prepared = deferred(); let stopped = false;
  const h = await harness(()=>prepared.promise);
  h.device.calls.length=0;
  const starting = h.host.start(h.mode('screen-average'),null);
  await h.host.stop();
  prepared.resolve({label:'Late',start(){throw Error('must not start');},async stop(){stopped=true;}});
  await starting;
  assert.equal(stopped,true);
  assert.equal(h.host.state.phase,'Idle');
  assert.equal(h.device.calls.length,0);
  await h.executor.close(); h.session.free();
});
test("host frame queue keeps latest sample and stop takes priority without backlog", async () => {
  const h = await harness();
  await h.host.start(h.mode('screen-average'),null);
  const blocked = deferred(), entered = deferred();
  h.device.beforeSend = async report => { if(report[0]===0x0e){entered.resolve();await blocked.promise;} };
  h.frame({Rgb:[1,2,3]});
  await entered.promise;
  for(let i=0;i<100;i++) h.frame({Rgb:[i,i,i]});
  const stopping = h.host.stop();
  blocked.resolve();
  await stopping;
  assert.equal(h.device.calls.filter(x=>x.kind==='send'&&x.report[0]===0x0e).length,1);
  assert.deepEqual([...h.device.lighting],h.original);
  await h.executor.close(); h.session.free();
});
test("failed host frame restores onboard state and keeps source error visible", async () => {
  const h = await harness();
  await h.host.start(h.mode('screen-average'),null);
  h.device.beforeSend = async report => {if(report[0]===0x0e) throw Error('injected frame failure');};
  h.frame({Rgb:[1,2,3]});
  await h.host.stop('capture lost');
  assert.equal(h.view().host.phase,'Idle');
  assert.deepEqual([...h.device.lighting],h.original);
  assert.match(h.host.state.problem,/frame failure|capture lost/);
  await h.executor.close(); h.session.free();
});

test("cancelled capture rejection cannot tear down a later host session", async () => {
  const late = deferred(); let calls = 0;
  const h = await harness(() => ++calls === 1 ? late.promise : Promise.resolve({label:'New source',start(){},async stop(){}}));
  const old = h.host.start(h.mode('screen-average'),null);
  await h.host.stop();
  await h.host.start(h.mode('screen-average'),null);
  assert.equal(h.view().host.phase,'Active');
  late.reject(new Error('old chooser rejected'));
  await old;
  assert.equal(h.view().host.phase,'Active');
  assert.equal(h.host.state.phase,'Active');
  await h.host.stop();
  assert.deepEqual([...h.device.lighting],h.original);
  await h.executor.close(); h.session.free();
});

test("late cancelled startup cannot clear a newer capture before restoration", async () => {
  const entered = deferred(), release = deferred();
  let captures = 0, stoppedNew = 0, blockedOnce = false;
  const prepare = async () => {
    const index = ++captures;
    return {label:`Capture ${index}`,start(){},async stop(){if(index===2) stoppedNew++;}};
  };
  const h = await harness(prepare, async (dispatch, input) => {
    if (input.type === "cancelCatalog" && !blockedOnce) {
      blockedOnce = true;
      entered.resolve();
      await release.promise;
    }
    return dispatch(input);
  });
  const old = h.host.start(h.mode("screen-average"), null);
  await entered.promise;
  await h.host.stop();
  await h.host.start(h.mode("screen-average"), null);
  assert.equal(h.view().host.phase, "Active");
  release.resolve();
  await old;
  await h.host.stop();
  assert.equal(stoppedNew, 1);
  assert.deepEqual([...h.device.lighting], h.original);
  await h.executor.close(); h.session.free();
});

test("overlapping stop requests share one capture shutdown and one restoration", async () => {
  const entered = deferred(), release = deferred();
  let stopped = 0;
  const prepare = async () => ({label:"Synthetic capture",start(){},async stop(){stopped++;entered.resolve();await release.promise;}});
  const h = await harness(prepare);
  await h.host.start(h.mode("screen-average"), null);
  h.device.calls.length = 0;
  const first = h.host.stop();
  await entered.promise;
  const second = h.host.stop();
  assert.strictEqual(first, second);
  release.resolve();
  await Promise.all([first, second]);
  assert.equal(stopped, 1);
  assert.equal(h.device.calls.filter(x => x.kind === "receive").length, 1);
  assert.deepEqual([...h.device.lighting], h.original);
  await h.executor.close(); h.session.free();
});
