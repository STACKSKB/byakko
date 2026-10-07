// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { layoutCatalog, layoutFor, defaultLayout } from "./tester/layouts.mjs";
import { emptyTest, updateTest, coverage } from "./tester/state.mjs";
import { boardBounds, keyRectangle } from "./key-geometry.mjs";

function footprints(key) {
  if (key.shape === "iso-enter") return [
    { x: key.x, y: key.y, width: key.width, height: 1 },
    { x: key.x + key.width / 2, y: key.y + 1, width: key.width / 2, height: 1 },
  ];
  return [key];
}

test("every physical/size/legend combination has unique keys and nonoverlapping footprints", () => {
  for (const standard of Object.keys(layoutCatalog.standards)) {
    for (const size of Object.keys(layoutCatalog.sizes)) {
      for (const legends of Object.keys(layoutCatalog.legends)) {
        const keys = layoutFor({ standard, size, legends });
        assert.equal(new Set(keys.map(key => key.id)).size, keys.length, `${standard}/${size}/${legends}`);
        const bounds = boardBounds(keys);
        for (const a of keys) {
          assert.ok(a.label.length > 0);
          assert.ok(a.x >= 0 && a.y >= 0 && a.width > 0 && a.height > 0);
          assert.ok(a.x + a.width <= bounds.width && a.y + a.height <= bounds.height);
          for (const b of keys) {
            if (a === b) continue;
            for (const p of footprints(a)) for (const q of footprints(b)) {
              assert.ok(p.x + p.width <= q.x || q.x + q.width <= p.x || p.y + p.height <= q.y || q.y + q.height <= p.y,
                `${standard}/${size}: ${a.id} overlaps ${b.id}`);
            }
          }
        }
      }
    }
  }
});

test("full-size ANSI/ISO/JIS contain the standard 104/105/109 keys and distinct numpad/modifiers", () => {
  for (const [standard, count] of [["ansi", 104], ["iso", 105], ["jis", 109]]) {
    const keys = layoutFor({ ...defaultLayout, standard });
    assert.equal(keys.length, count);
    for (const code of ["Enter", "NumpadEnter", "Digit1", "Numpad1", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"]) {
      assert.equal(keys.filter(key => key.code === code).length, 1);
    }
  }
  const jis = layoutFor({ ...defaultLayout, standard: "jis" });
  for (const code of ["IntlYen", "IntlRo", "NonConvert", "Convert", "KanaMode"]) assert.ok(jis.some(key => key.code === code));
  assert.ok(layoutFor({ ...defaultLayout, standard: "iso" }).some(key => key.code === "IntlBackslash"));
});

test("legends change labels while retaining physical identities and geometry", () => {
  const original = layoutFor(defaultLayout);
  const german = layoutFor({ ...defaultLayout, legends: "qwertz" });
  const french = layoutFor({ ...defaultLayout, legends: "azerty" });
  assert.equal(german.find(key => key.code === "KeyY").label, "Z");
  assert.equal(french.find(key => key.code === "KeyQ").label, "A");
  const geometry = keys => keys.map(({ label, ...key }) => key);
  assert.deepEqual(geometry(original), geometry(german));
  assert.deepEqual(geometry(original), geometry(french));
});

test("tester tracks separate physical keys, repeated input and release-only browser events", () => {
  const initial = emptyTest();
  const key = (code, pressed = true, repeat = false) => ({ type: "key", code, key: code, location: 0, repeat, pressed });
  let state = updateTest(initial, key("ShiftLeft"));
  state = updateTest(state, key("ShiftRight"));
  state = updateTest(state, key("Numpad1"));
  state = updateTest(state, key("Digit1"));
  state = updateTest(state, key("Digit1", true, true));
  assert.equal(state.held.size, 4);
  assert.equal(state.tested.size, 4);
  assert.equal(state.peak, 4);
  assert.equal(state.last.repeat, true);
  state = updateTest(state, key("Digit1", false));
  assert.equal(state.held.size, 3);
  assert.equal(state.tested.size, 4);
  state = updateTest(state, key("PrintScreen", false));
  assert.ok(state.tested.has("PrintScreen"));
  assert.ok(!state.held.has("PrintScreen"));
  assert.equal(initial.held.size, 0, "transitions leave previous state unchanged");
});

test("focus loss releases held keys; layout coverage preserves history and excludes firmware keys", () => {
  let state = emptyTest();
  for (const code of ["KeyA", "Numpad1", "Fn"]) {
    state = updateTest(state, { type: "key", code, key: code, location: 0, repeat: false, pressed: true });
  }
  const paused = updateTest(state, { type: "release-all" });
  assert.equal(paused.held.size, 0);
  assert.deepEqual(paused.tested, state.tested);
  assert.deepEqual(coverage(paused, layoutFor(defaultLayout)), { tested: 2, total: 104 });
  assert.deepEqual(coverage(paused, layoutFor({ ...defaultLayout, size: "60" })), { tested: 1, total: 60 });
  assert.deepEqual(updateTest(paused, { type: "reset" }), emptyTest());
});

test("unidentified input is reported without fabricating a physical key; Meta release clears swallowed releases", () => {
  let state = updateTest(emptyTest(), { type: "key", code: "", key: "あ", location: 0, repeat: false, pressed: true });
  assert.equal(state.last.key, "あ");
  assert.equal(state.tested.size, 0);
  for (const code of ["MetaLeft", "KeyL"]) {
    state = updateTest(state, { type: "key", code, key: code, location: 0, repeat: false, pressed: true });
  }
  state = updateTest(state, { type: "key", code: "MetaLeft", key: "Meta", location: 1, repeat: false, pressed: false });
  assert.equal(state.held.size, 0);
  assert.equal(state.tested.size, 2);
});

test("shared geometry retains existing configurator dimensions", () => {
  assert.deepEqual(keyRectangle({ x: 1.5, y: 2.5, width: 2, height: 1 }), { left: 71.5, top: 114.5, width: 82, height: 39 });
  assert.deepEqual(boardBounds([]), { width: 1, height: 1 });
});
