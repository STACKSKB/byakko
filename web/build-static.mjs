// SPDX-License-Identifier: GPL-3.0-or-later
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import { licenseInventory } from "./license-inventory.mjs";

const webRoot = dirname(fileURLToPath(import.meta.url));
const wasmDefault = "new URL('byakko_web_bg.wasm', import.meta.url)";
const logoAssignment = 'mark.src = "assets/byakko.svg";';

function replaceOnce(source, expected, replacement, label) {
  if (source.split(expected).length !== 2) {
    throw new Error(`Expected exactly one ${label}; the source or generated glue changed`);
  }
  return source.replace(expected, replacement);
}

function embeddedAssets(root) {
  const glue = resolve(root, "pkg/byakko_web.js");
  const app = resolve(root, "app.mjs");
  return {
    name: "byakko-embedded-assets",
    setup(bundle) {
      bundle.onLoad({ filter: /[\\/]pkg[\\/]byakko_web\.js$/ }, async args => {
        if (resolve(args.path) !== glue) return;
        const source = await readFile(args.path, "utf8");
        const contents = 'import __byakko_wasm_bytes from "./byakko_web_bg.wasm";\n' +
          replaceOnce(source, wasmDefault, "__byakko_wasm_bytes", "wasm-bindgen default WASM URL");
        return { contents, loader: "js", resolveDir: dirname(args.path) };
      });
      bundle.onLoad({ filter: /[\\/]app\.mjs$/ }, async args => {
        if (resolve(args.path) !== app) return;
        const source = await readFile(args.path, "utf8");
        const contents = 'import __byakko_logo from "./assets/byakko.svg";\n' +
          replaceOnce(source, logoAssignment, "mark.src = __byakko_logo;", "browser logo assignment");
        return { contents, loader: "js", resolveDir: dirname(args.path) };
      });
    },
  };
}

function attribute(tag, name) {
  const match = tag.match(new RegExp(`\\b${name}\\s*=\\s*(["'])(.*?)\\1`, "i"));
  return match?.[2];
}

function localFile(reference, label) {
  if (!reference || reference.startsWith("/") || reference.startsWith("\\") ||
      reference.includes("..") || /^[a-z][a-z\d+.-]*:/i.test(reference)) {
    throw new Error(`Expected a local ${label}, received ${reference}`);
  }
  return reference.split(/[?#]/, 1)[0];
}

function onlyOutput(result, extension) {
  if (result.outputFiles?.length !== 1 || !result.outputFiles[0].path.endsWith(extension)) {
    throw new Error(`Expected one bundled ${extension} output with no emitted assets`);
  }
  const outputs = Object.values(result.metafile.outputs);
  if (outputs.length !== 1 || outputs[0].imports.length !== 0) {
    throw new Error(`Bundled ${extension} still has an external dependency`);
  }
  return result.outputFiles[0].text;
}

function inlineSafe(contents, tag) {
  return contents.replace(new RegExp(`</${tag}`, "gi"), `<\\/${tag}`);
}

export async function buildStatic({ root = webRoot, html = "index.html", output = "dist/index.html" } = {}) {
  root = resolve(root);
  const htmlPath = resolve(root, html);
  const outputPath = isAbsolute(output) ? output : resolve(root, output);
  const page = await readFile(htmlPath, "utf8");
  const scripts = [...page.matchAll(/<script\b[^>]*><\/script\s*>/gi)];
  const stylesheets = [...page.matchAll(/<link\b[^>]*>/gi)].filter(match => attribute(match[0], "rel") === "stylesheet");
  if (scripts.length !== 1 || stylesheets.length !== 1 ||
      attribute(scripts[0][0], "type") !== "module") {
    throw new Error(`${html} needs one module script and one stylesheet`);
  }
  const entry = localFile(attribute(scripts[0][0], "src"), "module script");
  const style = localFile(attribute(stylesheets[0][0], "href"), "stylesheet");
  const common = {
    absWorkingDir: root,
    bundle: true,
    write: false,
    metafile: true,
    logLevel: "silent",
    legalComments: "inline",
    target: "es2022",
  };
  const javascript = onlyOutput(await build({
    ...common,
    entryPoints: [entry],
    outfile: "__byakko_bundle.js",
    platform: "browser",
    format: "esm",
    splitting: false,
    loader: { ".wasm": "binary", ".svg": "dataurl" },
    plugins: [embeddedAssets(root)],
  }), ".js");
  const css = onlyOutput(await build({
    ...common,
    entryPoints: [style],
    outfile: "__byakko_bundle.css",
    loader: { ".svg": "dataurl" },
  }), ".css");
  if (/@import\b/i.test(css)) throw new Error("Bundled CSS still imports another stylesheet");
  for (const url of css.matchAll(/url\(\s*([^)]+)\)/gi)) {
    const value = url[1].trim().replace(/^["']|["']$/g, "");
    if (!value.startsWith("data:") && !value.startsWith("#")) {
      throw new Error(`Bundled CSS still refers to ${value}`);
    }
  }
  const standalone = page
    .replace("</head>", '<link rel="icon" href="data:,">\n</head>')
    .replace(stylesheets[0][0], () => `<style>\n/* SPDX-License-Identifier: GPL-3.0-or-later */\n${inlineSafe(css, "style")}\n</style>`);
  const notices = JSON.stringify(await licenseInventory()).replaceAll("<", "\\u003c");
  const result = standalone.replace(scripts[0][0], () => `<script id="byakko-notices" type="application/json">${notices}</script>\n<script type="module">\n// SPDX-License-Identifier: GPL-3.0-or-later\n${inlineSafe(javascript, "script")}\n</script>`);
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, result);
  return outputPath;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  buildStatic().then(path => console.log(`Built ${path}`)).catch(error => {
    console.error(error);
    process.exitCode = 1;
  });
}
