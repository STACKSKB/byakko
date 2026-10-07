// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { gzipSync } from "node:zlib";
import { buildTester } from "./build-tester.mjs";

test("tester builds standalone and indexable WordPress fragment without configurator runtime", async () => {
  const destination = await mkdtemp(join(tmpdir(), "byakko-tester-build-"));
  await buildTester({ destination });
  const [html, fragment, js, css] = await Promise.all(["index.html", "tester.html", "tester.js", "tester.css"].map(file => readFile(join(destination, file), "utf8")));
  assert.match(html, /<h1 class="bt-title">Keyboard tester<\/h1>/);
  assert.match(fragment, /<h2 class="bt-title">Keyboard tester<\/h2>/);
  assert.equal((fragment.match(/data-code=/g) ?? []).length, 104);
  assert.match(fragment, /How to use this keyboard tester/);
  assert.doesNotMatch(fragment, /<script|<style|<iframe|<!-- INITIAL_BOARD -->/);
  assert.doesNotMatch(html, /(?:src|href)="(?!data:)[^"]+"/);
  assert.doesNotMatch(js, /navigator\.hid|WebAssembly|indexedDB|localStorage|fetch\(|XMLHttpRequest|WebSocket/);
  assert.ok(gzipSync(js).length + gzipSync(css).length < 30 * 1024);
  assert.ok(gzipSync(html).length < 30 * 1024, "entire initial HTML fits the compressed budget");
  const selectors = [...css.matchAll(/(?:^|})([^{}]+){/g)].map(match => match[1].trim());
  assert.ok(selectors.every(selector => selector.startsWith(".byakko-tester") || selector.startsWith("@media")), "styles are scoped to the tester");
});
