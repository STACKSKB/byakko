// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { prepareSampler, meanRgb, pointCoordinate } from "./sampler.mjs";
test("screen sampling averages channels, excludes alpha and maps native normalized endpoints", () => {
  assert.deepEqual(meanRgb([255, 0, 0, 0, 0, 255, 0, 255]), [127, 127, 0]);
  assert.equal(pointCoordinate(0, 1920), 0);
  assert.equal(pointCoordinate(1000, 1920), 1919);
  assert.equal(pointCoordinate(500, 1920), 959);
  assert.throws(() => pointCoordinate(1001, 1920));
  assert.throws(() => meanRgb([]));
});
test("absent shared audio closes capture and cannot silently substitute microphone", async () => {
  let stopped = false;
  const env = { navigator: { mediaDevices: { getDisplayMedia: async options => {
    assert.equal(options.audio, true);
    return { getTracks: () => [{ stop() { stopped = true; } }], getAudioTracks: () => [] };
  } } }, clearTimeout() {} };
  await assert.rejects(prepareSampler({PlaybackAudio:{bands:32}}, {}, {}, env), /no audio/);
  assert.equal(stopped, true);
});

test("capture ending during keyboard startup is retained until sampling starts", async () => {
  const track = Object.assign(new EventTarget(), {readyState:'live',label:'Synthetic audio',stop(){this.readyState='ended';}});
  const stream = {getTracks:()=>[track],getAudioTracks:()=>[track]};
  const env = { navigator:{mediaDevices:{getDisplayMedia:async()=>stream}},clearTimeout(){},setTimeout(){throw Error('must not sample');},
    AudioContext: class {
      state='running';sampleRate=48000;
      async resume(){} async close(){this.state='closed';}
      createAnalyser(){return {fftSize:2048,getFloatTimeDomainData(values){values.fill(0);}};}
      createMediaStreamSource(){return {connect(){},disconnect(){}};}
    },
  };
  const codec = {BrowserAudioBands:class {free(){} frame(){return Array(32).fill(0);}}};
  const sampler = await prepareSampler({PlaybackAudio:{bands:32}}, {}, codec, env);
  track.readyState='ended'; track.dispatchEvent(new Event('ended'));
  let failure;
  sampler.start(()=>assert.fail('no frame from ended source'), reason=>failure=reason);
  assert.match(failure,/ended before startup/);
  await sampler.stop();
});

test("overlapping audio capture stops close tracks and context only once", async () => {
  let closeCount = 0, trackStops = 0, releaseClose;
  const closing = new Promise(resolve => { releaseClose = resolve; });
  const track = {readyState:"live",label:"Shared audio",stop(){trackStops++;this.readyState="ended";},addEventListener(){}};
  const stream = {getTracks:()=>[track],getAudioTracks:()=>[track]};
  const env = {
    navigator:{mediaDevices:{getDisplayMedia:async()=>stream}},clearTimeout(){},
    AudioContext: class {
      state="running";sampleRate=48000;
      async resume(){}
      close(){closeCount++;return closing;}
      createAnalyser(){return {fftSize:2048};}
      createMediaStreamSource(){return {connect(){},disconnect(){}};}
    },
  };
  const codec = {BrowserAudioBands:class {free(){}}};
  const sampler = await prepareSampler({PlaybackAudio:{bands:32}}, {}, codec, env);
  const first = sampler.stop(), second = sampler.stop();
  assert.strictEqual(first, second);
  await Promise.resolve();
  assert.equal(trackStops, 1);
  assert.equal(closeCount, 1);
  releaseClose();
  await Promise.all([first, second]);
  assert.equal(closeCount, 1);
});
