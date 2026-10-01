// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildStatic } from "./build-static.mjs";
import { buildWordpress } from "./build-wordpress.mjs";

test("WordPress package contains the exact standalone client, wrapper, requirements and licenses", async t => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-wordpress-test-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const bundle = join(scratch, "index.html");
  await buildStatic({ output: bundle });
  const plugin = await buildWordpress({ bundle, destination: join(scratch, "wordpress") });
  assert.deepEqual((await readdir(plugin)).sort(), ["LICENSE", "README.txt", "byakko-configurator.php", "embed.js", "index.html", "linux-requirements.html"]);
  assert.deepEqual(await readFile(join(plugin, "index.html")), await readFile(bundle));
  assert.deepEqual(await readFile(join(plugin, "LICENSE")), await readFile(new URL("../LICENSE", import.meta.url)));
});
