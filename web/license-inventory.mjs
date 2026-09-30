// SPDX-License-Identifier: GPL-3.0-or-later
// Build-only notice collection. Never imported by the browser entry point.
import { execFile } from "node:child_process";
import { readFile, readdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execute = promisify(execFile);
const root = fileURLToPath(new URL("../", import.meta.url));

export async function licenseInventory() {
  const {stdout} = await execute("cargo", ["metadata", "--locked", "--offline",
    "--format-version", "1", "--filter-platform", "wasm32-unknown-unknown"],
  {cwd: root, maxBuffer: 16 * 1024 * 1024});
  const metadata = JSON.parse(stdout);
  const packages = new Map(metadata.packages.map(item => [item.id, item]));
  const nodes = new Map(metadata.resolve.nodes.map(item => [item.id, item]));
  const browser = metadata.packages.find(item => item.name === "byakko-web");
  const reachable = new Set(), pending = [browser.id];
  while (pending.length) {
    const id = pending.pop();
    if (reachable.has(id)) continue;
    reachable.add(id);
    pending.push(...nodes.get(id).dependencies);
  }
  const rows = [];
  for (const item of [...reachable].map(id => packages.get(id)).sort((a, b) => a.name.localeCompare(b.name))) {
    // Byakko-owned crates use the repository's GPL notice below.
    if (!item.source) continue;
    const source = dirname(item.manifest_path);
    const entries = await readdir(source, {withFileTypes: true});
    const filenames = new Set(entries.filter(entry => entry.isFile() &&
      /^(license|notice|copying|copyright)/i.test(entry.name)).map(entry => entry.name));
    if (item.license_file) filenames.add(item.license_file);
    if (!filenames.size) throw new Error(`Missing third-party notices: ${item.name} ${item.version}`);
    const notices = {};
    for (const filename of [...filenames].sort()) notices[filename] = await readFile(join(source, filename), "utf8");
    rows.push({name:item.name, version:item.version, license:item.license, notices});
  }
  // esbuild's binary loader contributes a small generated decoding helper.
  const buildPackage = JSON.parse(await readFile(new URL("./node_modules/esbuild/package.json", import.meta.url), "utf8"));
  rows.push({name:"esbuild", version:buildPackage.version, license:buildPackage.license,
    notices:{"LICENSE.md":await readFile(new URL("./node_modules/esbuild/LICENSE.md", import.meta.url), "utf8")}});
  return {license:await readFile(join(root, "LICENSE"), "utf8"), packages:rows};
}
