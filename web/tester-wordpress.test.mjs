// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildTesterWordpress } from "./build-tester-wordpress.mjs";

const run = promisify(execFile);

test("tester ZIP contains the standalone build, WordPress wrapper, and editable source", async () => {
  const scratch = await mkdtemp(join(tmpdir(), "byakko-tester-wp-"));
  const { plugin, zip } = await buildTesterWordpress({ destination: scratch });
  const { stdout } = await run("python3", ["-c", `
import json, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    print(json.dumps(sorted(archive.namelist())))
`, zip]);
  const files = JSON.parse(stdout);
  for (const file of [
    "LICENSE", "README.txt", "byakko-keyboard-tester.php", "index.html",
    "tester.html", "tester.js", "tester.css", "source/web/build-tester.mjs",
    "source/web/build-tester-wordpress.mjs", "source/web/input.mjs",
    "source/web/key-geometry.mjs", "source/web/tester/layouts.json",
    "source/web/tester.test.mjs", "source/web/tester-build.test.mjs", "source/web/tester-browser.test.mjs",
    "source/web/tester-wordpress.test.mjs", "source/docs/keyboard-tester.md",
    "source/LICENSE",
  ]) {
    assert.ok(files.includes(`byakko-keyboard-tester/${file}`), file);
  }
  assert.deepEqual(await readFile(join(plugin, "LICENSE")), await readFile(new URL("../LICENSE", import.meta.url)));
  assert.match(await readFile(join(plugin, "index.html"), "utf8"), /data-byakko-tester/);
  assert.match(await readFile(join(plugin, "tester.html"), "utf8"), /Keyboard tester/i);

  const harness = join(scratch, "wordpress-harness.php");
  await writeFile(harness, `<?php
define('ABSPATH', '/wordpress/');
class WP_Post { public function __construct(public $post_content) {} }
$actions = []; $shortcodes = []; $styles = []; $scripts = []; $current = null;
function add_action($name, $callback) { global $actions; $actions[$name] = $callback; }
function add_shortcode($name, $callback) { global $shortcodes; $shortcodes[$name] = $callback; }
function get_queried_object() { global $current; return $current; }
function has_shortcode($content, $name) { return str_contains($content, '[' . $name . ']'); }
function plugins_url($file, $plugin) { return '/plugins/byakko-keyboard-tester/' . $file; }
function wp_enqueue_style(...$args) { global $styles; $styles[] = $args; }
function wp_enqueue_script(...$args) { global $scripts; $scripts[] = $args; }
function esc_html__($text, $domain) { return htmlspecialchars($text, ENT_QUOTES); }
require $argv[1];
if (!isset($actions['wp_enqueue_scripts'], $shortcodes['byakko_keyboard_tester'])) throw new RuntimeException('Hooks missing');
$current = new WP_Post('A page without the tester');
$actions['wp_enqueue_scripts']();
if ($styles || $scripts) throw new RuntimeException('Unrelated page loaded tester assets');
$current = new WP_Post('[byakko_keyboard_tester] [byakko_keyboard_tester]');
$actions['wp_enqueue_scripts'](); // WordPress calls this before wp_head.
if (count($styles) !== 1 || count($scripts) !== 1 || !$scripts[0][4]) throw new RuntimeException('Head/footer assets missing');
$html = $shortcodes['byakko_keyboard_tester']() . $shortcodes['byakko_keyboard_tester']();
if (substr_count($html, 'data-byakko-tester') !== 2) throw new RuntimeException('Multiple instances failed');
if (preg_match('/\\bid\\s*=/', $html)) throw new RuntimeException('Duplicate IDs in tester fragment');
echo json_encode(['html' => $html, 'styles' => $styles, 'scripts' => $scripts]);
`);
  const php = await run("php", [harness, join(plugin, "byakko-keyboard-tester.php")]);
  const page = JSON.parse(php.stdout);
  assert.match(page.html, /Keyboard tester/i);
  assert.equal(page.styles[0][1], "/plugins/byakko-keyboard-tester/tester.css");
  assert.equal(page.scripts[0][1], "/plugins/byakko-keyboard-tester/tester.js");
});
