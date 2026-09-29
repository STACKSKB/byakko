import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { bindingLabel, bindingChanges } from "./keymap_view.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

test("actual Nia87 descriptor projects remapped board label and visible before/after", () => {
  const session = new codec.BrowserSession();
  try {
    const descriptor = JSON.parse(session.view()).descriptor;
    const base = descriptor.layers[0];
    const key = descriptor.keys.find(item => item.visible && item.writable && !base.read_only_keys.includes(item.id));
    const before = { Key: 4 }, after = { Key: 5 };
    const from = bindingLabel(descriptor, before, base.id, key.id);
    const to = bindingLabel(descriptor, after, base.id, key.id);
    assert.equal(from.full, "A");
    assert.equal(to.full, "B");
    assert.deepEqual(bindingChanges(descriptor,
      { bindings: { [base.id]: { [key.id]: before } } },
      { [base.id]: { [key.id]: after } }), [
      { layer: base.label, key: key.label, before: "A", after: "B" },
    ]);
  } finally { session.free(); }
});

test("shortcut, local macro name and protected opaque labels retain context", () => {
  const descriptor = {
    layers: [{ id: "fn", label: "Fn", read_only_keys: ["system"] }],
    keys: [{ id: "system", label: "System" }], actions: [],
    shortcuts: { modifiers: [{ usage: 224, label: "Ctrl" }], keys: [{ usage: 4, label: "A" }] },
  };
  assert.equal(bindingLabel(descriptor, { Shortcut: { modifiers: [224], key: 4 } }, "fn", "system").full, "Ctrl+A");
  assert.equal(bindingLabel(descriptor, { Macro: { slot: 3, mode: 0 } }, "fn", "system", { "slot-03": "Launch" }).full, "Launch");
  assert.equal(bindingLabel(descriptor, { Opaque: { label: "Vendor command" } }, "fn", "system").compact, "Onboard");
});
