// SPDX-License-Identifier: GPL-3.0-or-later
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { build } from "esbuild";
import { layoutFor, defaultLayout } from "./tester/layouts.mjs";
import { boardBounds, keyRectangle } from "./key-geometry.mjs";

const webRoot = dirname(fileURLToPath(import.meta.url));
const escape = value => String(value).replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
const inline = (value, tag) => value.replace(new RegExp(`</${tag}`, "gi"), `<\\/${tag}`);

export async function buildTester({ destination = resolve(webRoot, "dist/tester") } = {}) {
  const common = { absWorkingDir: webRoot, bundle: true, write: false, minify: true,
    legalComments: "inline", target: "es2020", logLevel: "silent", metafile: true };
  const javascript = await build({ ...common, entryPoints: ["tester/tester.mjs"], format: "iife", platform: "browser" });
  const stylesheet = await build({ ...common, entryPoints: ["tester/tester.css"] });
  for (const output of [javascript, stylesheet]) {
    if (output.outputFiles.length !== 1 || Object.values(output.metafile.outputs).some(file => file.imports.length)) {
      throw new Error("Tester must have no external runtime dependencies");
    }
  }
  const js = javascript.outputFiles[0].text;
  const css = stylesheet.outputFiles[0].text;
  const bytes = gzipSync(js).length + gzipSync(css).length;
  if (bytes > 30 * 1024) throw new Error(`Tester assets exceed the 30 KiB compressed budget: ${bytes}`);
  const keys = layoutFor(defaultLayout);
  const bounds = boardBounds(keys);
  const picture = keys.map(key => {
    const style = Object.entries(keyRectangle(key)).map(([name, value]) => `${name}:${value}px`).join(";");
    return `<span class="bt-key" data-code="${escape(key.code)}" title="${escape(key.code)}" style="${style}">${escape(key.label)}</span>`;
  }).join("");
  const source = await readFile(resolve(webRoot, "tester/template.html"), "utf8");
  const fragment = source.replace('class="bt-board"', `class="bt-board" style="width:${bounds.width * 43 + 14}px;height:${bounds.height * 43 + 14}px"`).replace("<!-- INITIAL_BOARD -->", picture);
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Keyboard tester — ANSI, ISO and JIS layouts</title>
<meta name="description" content="Test keyboard input with full-size, TKL, 75%, 65% and 60% layouts. See held keys, tested keys and browser events. No installation or USB permissions needed.">
<link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect x='2' y='6' width='28' height='20' rx='3' fill='%23565b8f'/%3E%3Cpath d='M7 11h3m3 0h3m3 0h3M7 16h3m3 0h3m3 0h3M9 21h14' stroke='white' stroke-width='2'/%3E%3C/svg%3E"><style>${inline(css, "style")}</style></head>
<body style="margin:0;padding:24px 0;font-family:system-ui,sans-serif;background:#fff">
${fragment.replace('<h2 class="bt-title">', '<h1 class="bt-title">').replace('</h2>', '</h1>')}
<script>${inline(js, "script")}</script></body></html>`;
  destination = resolve(destination);
  await mkdir(destination, { recursive: true });
  await Promise.all([
    writeFile(resolve(destination, "index.html"), html),
    writeFile(resolve(destination, "tester.html"), fragment),
    writeFile(resolve(destination, "tester.js"), js),
    writeFile(resolve(destination, "tester.css"), css),
  ]);
  return destination;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  buildTester().then(path => console.log(`Built ${path}`)).catch(error => {
    console.error(error); process.exitCode = 1;
  });
}
