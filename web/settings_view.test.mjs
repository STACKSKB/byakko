import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as codec from "./pkg/byakko_web.js";
import { pendingField, numberKind, needsInitialRead } from "./settings_view.mjs";

await codec.default({ module_or_path: await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url)) });

const baseline = { content: { Editable: {
  automatic_os: { Toggle: true },
  bluetooth_deep_sleep: { Number: 0 },
} } };

test("settings presentation follows the core's single pending field", () => {
  const editor = { baseline, draft: baseline.content.Editable, submitted: null };
  assert.equal(pendingField(editor), null);
  editor.draft = { ...editor.draft, bluetooth_deep_sleep: { Number: 10 } };
  assert.equal(pendingField(editor), "bluetooth_deep_sleep");
  editor.submitted = editor.draft;
  editor.draft = { ...editor.draft, automatic_os: { Toggle: false } };
  assert.equal(pendingField(editor), "bluetooth_deep_sleep");
});

test("numeric constraints are read from advertised capability", () => {
  assert.deepEqual(numberKind({ kind: { Number: { min: 10, max: 60, step: 1, disabled_zero: true } } }),
    { min: 10, max: 60, step: 1, disabled_zero: true });
  assert.equal(numberKind({ kind: "Toggle" }), null);
});

test("connected WASM settings require one initial read and later failures do not auto-retry", () => {
  const session = new codec.BrowserSession();
  try {
    const connected = JSON.parse(session.dispatch('{"type":"connect"}'));
    assert.equal(connected.ok, true);
    const editor = connected.view.settings.editor;
    assert.deepEqual(editor.status, { Unverified: { problem: "ReadRequired" } });
    assert.equal(editor.canRead, true);
    assert.equal(needsInitialRead(editor), true);
    assert.equal(needsInitialRead({ ...editor, status: { Unverified: { problem: { Read: "device failed" } } } }), false);
    assert.equal(needsInitialRead({ ...editor, baseline: { content: { Editable: {} } } }), false);
  } finally { session.free(); }
});
