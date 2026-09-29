import test from "node:test";
import assert from "node:assert/strict";
import { usageForCode, recordedKey, pointerButtonUsage, recordedPointer, recordingPolicy } from "./input.mjs";

test("physical keys retain distinct modifier sides and ignore unidentified codes", () => {
  assert.equal(usageForCode("KeyA"), 4);
  assert.equal(usageForCode("ControlLeft"), 0xe0);
  assert.equal(usageForCode("ControlRight"), 0xe4);
  assert.equal(usageForCode("F13"), 0x68);
  assert.equal(usageForCode("F24"), 0x73);
  assert.equal(usageForCode("IntlYen"), 0x89);
  assert.deepEqual(recordedKey({ code: "KeyA" }, false), { Key: { usage: 4, pressed: false } });
  assert.equal(recordedKey({ code: "Unidentified" }, true), null);
});

test("pointer buttons use HID order and only advertised buttons are recorded", () => {
  assert.deepEqual([0, 1, 2, 3, 4].map(pointerButtonUsage), [1, 3, 2, 4, 5]);
  const caps = { buttons: [{ button: 1 }, { button: 3 }] };
  assert.deepEqual(recordedPointer({ button: 1 }, true, caps), { Button: { button: 3, pressed: true } });
  assert.equal(recordedPointer({ button: 2 }, true, caps), null);
});

test("recording modes preserve measured terminal wait and validate fixed delay", () => {
  const caps = { delays_ms: { start: 0, end: 100 } };
  assert.deepEqual(recordingPolicy({ fixed: false, delay: "" }, caps), { Measured: { terminal_ms: 50 } });
  assert.deepEqual(recordingPolicy({ fixed: true, delay: "20" }, caps), { Fixed: 20 });
  assert.throws(() => recordingPolicy({ fixed: true, delay: "0" }, caps));
  assert.throws(() => recordingPolicy({ fixed: true, delay: "101" }, caps));
  assert.throws(() => recordingPolicy({ fixed: true, delay: "1e2" }, caps));
});
