// SPDX-License-Identifier: GPL-3.0-or-later
// Build an uploadable WordPress plugin. Node and Python are build-time tools only.
import { spawn } from "node:child_process";
import { copyFile, mkdir, readdir } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildTester } from "./build-tester.mjs";

const webRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(webRoot, "..");

async function copyInto(source, target) {
  await mkdir(dirname(target), { recursive: true });
  await copyFile(source, target);
}

async function sourceFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...await sourceFiles(path));
    else if (entry.isFile()) files.push(path);
  }
  return files;
}

function zipPlugin(destination, files) {
  // Fixed timestamps, order, and file modes make repeated builds byte-identical.
  const script = `import pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
output = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for name in sorted(sys.argv[3:]):
        path = root / name
        info = zipfile.ZipInfo(path.relative_to(root).as_posix(), (1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        archive.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
`;
  return new Promise((accept, reject) => {
    const child = spawn(process.env.PYTHON || "python3", ["-c", script, destination, resolve(destination, "byakko-keyboard-tester.zip"), ...files.map(file => relative(destination, file))], { stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", code => code === 0 ? accept() : reject(new Error(`ZIP creation exited ${code}`)));
  });
}

export async function buildTesterWordpress({ destination = resolve(webRoot, "dist/wordpress-tester") } = {}) {
  const output = resolve(destination);
  const compiled = resolve(output, "_tester_build");
  const plugin = resolve(output, "byakko-keyboard-tester");
  await mkdir(output, { recursive: true });
  await buildTester({ destination: compiled });
  await mkdir(plugin, { recursive: true });
  const files = [];
  async function include(source, target) {
    await copyInto(source, target);
    files.push(target);
  }

  for (const file of ["index.html", "tester.html", "tester.js", "tester.css"]) {
    await include(resolve(compiled, file), resolve(plugin, file));
  }
  for (const file of ["byakko-keyboard-tester.php", "README.txt"]) {
    await include(resolve(webRoot, "tester/wordpress", file), resolve(plugin, file));
  }
  await include(resolve(repoRoot, "LICENSE"), resolve(plugin, "LICENSE"));

  // The distributed binary assets can be rebuilt and modified from this source.
  const source = resolve(plugin, "source");
  for (const file of await sourceFiles(resolve(webRoot, "tester"))) {
    await include(file, resolve(source, "web", relative(webRoot, file)));
  }
  for (const file of ["build-tester.mjs", "build-tester-wordpress.mjs", "input.mjs", "key-geometry.mjs", "package.json", "package-lock.json", "tester.test.mjs", "tester-build.test.mjs", "tester-browser.test.mjs", "tester-wordpress.test.mjs"]) {
    await include(resolve(webRoot, file), resolve(source, "web", file));
  }
  await include(resolve(repoRoot, "docs/keyboard-tester.md"), resolve(source, "docs/keyboard-tester.md"));
  await include(resolve(repoRoot, "LICENSE"), resolve(source, "LICENSE"));
  await zipPlugin(output, files);
  return { plugin, zip: resolve(output, "byakko-keyboard-tester.zip") };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  buildTesterWordpress().then(({ zip }) => console.log(`Built ${zip}`)).catch(error => {
    console.error(error);
    process.exitCode = 1;
  });
}
