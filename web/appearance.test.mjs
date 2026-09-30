// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { preference, resolvedTheme } from "./appearance.mjs";

test("appearance preferences reject unknown persisted values and follow system only when selected", () => {
  assert.deepEqual(preference(null), {mode:"system",palette:"indigo"});
  assert.deepEqual(preference({mode:"invalid",palette:"invalid"}), {mode:"system",palette:"indigo"});
  assert.deepEqual(preference({mode:"dark",palette:"forest"}), {mode:"dark",palette:"forest"});
  assert.equal(resolvedTheme("system", true), "dark");
  assert.equal(resolvedTheme("system", false), "light");
  assert.equal(resolvedTheme("light", true), "light");
  assert.equal(resolvedTheme("dark", false), "dark");
});
