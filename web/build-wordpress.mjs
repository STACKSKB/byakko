// SPDX-License-Identifier: GPL-3.0-or-later
// Optional local packaging. The WordPress server receives no Node tooling.
import { copyFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = dirname(fileURLToPath(import.meta.url));

export async function buildWordpress({
  bundle = resolve(webRoot, "dist/index.html"),
  destination = resolve(webRoot, "dist/wordpress"),
} = {}) {
  const plugin = resolve(destination, "byakko-configurator");
  await mkdir(plugin, { recursive: true });
  await copyFile(bundle, resolve(plugin, "index.html"));
  for (const filename of ["byakko-configurator.php", "README.txt"]) {
    await copyFile(resolve(webRoot, "wordpress", filename), resolve(plugin, filename));
  }
  await copyFile(resolve(webRoot, "../LICENSE"), resolve(plugin, "LICENSE"));
  return plugin;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  buildWordpress().then(path => console.log(`Built ${path}`)).catch(error => {
    console.error(error);
    process.exitCode = 1;
  });
}
