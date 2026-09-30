// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildStatic } from "./build-static.mjs";

test("each browser entry builds into one self-contained HTML file", async t => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-static-test-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const wasm = await readFile(new URL("./pkg/byakko_web_bg.wasm", import.meta.url));
  const encodedWasm = wasm.toString("base64");
  for (const html of ["index.html", "app-tests.html", "test.html"]) {
    const output = join(scratch, html);
    await buildStatic({ html, output });
    const page = await readFile(output, "utf8");
    assert.match(page, /<style>/);
    assert.match(page, /<script type="module">/);
    assert.doesNotMatch(page, /<link\b[^>]*rel=["']stylesheet["']/i);
    assert.doesNotMatch(page, /<(?:script|img)\b[^>]*\bsrc=/i);
    assert.doesNotMatch(page, /new URL\(['"]byakko_web_bg\.wasm['"]/);
    assert.ok(page.includes(encodedWasm), `${html} does not contain the exact generated WASM bytes`);
    assert.match(page, /data:image\/svg\+xml[,;]/);
    assert.equal([...page.matchAll(/<\/script>/gi)].length, 2);
    const notices = page.match(/<script id="byakko-notices" type="application\/json">([^<]+)<\/script>/);
    assert.ok(notices, `${html} lacks embedded license notices`);
    const inventory = JSON.parse(notices[1]);
    assert.match(inventory.license, /GNU GENERAL PUBLIC LICENSE/);
    assert.ok(inventory.packages.some(packageNotice => packageNotice.name === "esbuild"));
  }
});

test("changed wasm-bindgen URL signature fails instead of leaving a WASM fetch", async t => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-static-glue-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  await mkdir(join(scratch, "pkg"));
  await writeFile(join(scratch, "index.html"), '<link rel="stylesheet" href="style.css"><script type="module" src="main.mjs"></script>');
  await writeFile(join(scratch, "style.css"), "body { color: black; }");
  await writeFile(join(scratch, "main.mjs"), 'import init from "./pkg/byakko_web.js"; init();');
  await writeFile(join(scratch, "pkg/byakko_web.js"), "export default function init() { return new URL('different.wasm', import.meta.url); }");
  await assert.rejects(buildStatic({ root: scratch, output: join(scratch, "dist/index.html") }),
    /wasm-bindgen default WASM URL/);
});

test("inline content escapes HTML script and style terminators", async t => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-static-escape-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  await writeFile(join(scratch, "index.html"), '<link rel="stylesheet" href="style.css"><script type="module" src="main.mjs"></script>');
  await writeFile(join(scratch, "style.css"), 'body::after { content: "</style>"; }');
  await writeFile(join(scratch, "main.mjs"), 'globalThis.inlineText = "</script>";');
  const output = join(scratch, "dist/index.html");
  await buildStatic({ root: scratch, output });
  const page = await readFile(output, "utf8");
  assert.match(page, /<\\\/script>/);
  assert.match(page, /<\\\/style>/);
  assert.equal([...page.matchAll(/<\/script>/g)].length, 2);
  assert.equal([...page.matchAll(/<\/style>/g)].length, 1);
});

test("embedding keeps JavaScript and CSS replacement tokens literal", async t => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-static-literal-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  await writeFile(join(scratch, "index.html"), '<link rel="stylesheet" href="style.css"><script type="module" src="main.mjs"></script>');
  const literal = "$& $` $' $$";
  await writeFile(join(scratch, "style.css"), `body::after { content: ${JSON.stringify(literal)}; }`);
  await writeFile(join(scratch, "main.mjs"), `globalThis.inlineText = ${JSON.stringify(literal)};`);
  const output = join(scratch, "dist/index.html");
  await buildStatic({ root: scratch, output });
  const page = await readFile(output, "utf8");
  assert.equal(page.split(literal).length, 3);
  assert.doesNotMatch(page, /\bsrc="main\.mjs"/);
  assert.doesNotMatch(page, /\bhref="style\.css"/);
});
