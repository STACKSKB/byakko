// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { preference, resolvedTheme, statusColors, validCustomColors } from "./appearance.mjs";

test("appearance preferences reject unknown persisted values and follow system only when selected", () => {
  const blank = preference(null);
  assert.equal(blank.mode, "system");
  assert.equal(blank.palette, "indigo");
  assert(validCustomColors(blank.custom.light) && validCustomColors(blank.custom.dark));
  assert.equal(preference({mode:"invalid",palette:"invalid"}).palette, "indigo");
  assert.equal(preference({mode:"dark",palette:"forest"}).mode, "dark");
  assert.deepEqual(
    {mode:preference(null, {mode:"light",palette:"embed"}).mode,palette:preference(null, {mode:"light",palette:"embed"}).palette},
    {mode:"light",palette:"embed"},
  );
  assert.equal(preference({mode:"dark",palette:"embed"}).palette, "embed");
  assert.equal(preference({mode:"light",palette:"stackskb"}).palette, "stackskb");
  assert.equal(resolvedTheme("system", true), "dark");
  assert.equal(resolvedTheme("system", false), "light");
  assert.equal(resolvedTheme("light", true), "light");
  assert.equal(resolvedTheme("dark", false), "dark");
});

test("custom colours restore only complete readable palettes", () => {
  const light = {...preference(null).custom.light, canvas:"#F9F9F9", focus:"#215387"};
  assert(validCustomColors(light));
  assert.equal(preference({custom:{light}}).custom.light.canvas, "#f9f9f9");
  assert.equal(preference({custom:{light:{...light, ink:"#f9f9f9"}}}).custom.light.ink, "#272823");
  assert.equal(preference({custom:{light:{...light, focus:"url(foo)"}}}).custom.light.focus, "#565b8f");
  assert.equal(preference({custom:{light:{...light, muted:"#eeeeee"}}}).custom.light.muted, "#686962");
});

test("custom surfaces keep semantic status text readable", () => {
  const colors = {canvas:"#a4382c",paper:"#a4382c",ink:"#ffffff",muted:"#ffffff",accent:"#ffffff",focus:"#ffffff",rule:"#ffffff"};
  assert(validCustomColors(colors));
  assert.equal(statusColors(colors).error, "#ffffff");
  assert.equal(statusColors(colors).success, "#ffffff");
});

test("StackKB palette keeps official light tokens and the neutral embedded defaults", async () => {
  const css = await readFile(new URL("./palette.css", import.meta.url), "utf8");
  const official = css.match(/:root\[data-palette="stackskb"\] \{([^}]+)\}/)?.[1];
  assert(official);
  for (const color of ["#f7f7f7","#ffffff","#000000","#505050","#ededed","#2b6cb0","#464646"]) {
    assert(official.includes(color), `Missing StacksKB colour ${color}`);
  }
  assert.match(css, /:root\[data-palette="embed"\][\s\S]*?--canvas: #ffffff; --paper: #ffffff/);
});

test("actual palette CSS keeps disabled and physical key text readable", async () => {
  const palette = await readFile(new URL("./palette.css", import.meta.url), "utf8");
  const style = await readFile(new URL("./style.css", import.meta.url), "utf8");
  const variables = selector => Object.fromEntries(
    [...palette.matchAll(new RegExp(`${selector} \\{([^}]+)\\}`, "g"))]
      .flatMap(match => [...match[1].matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/gi)])
      .map(([, name, value]) => [name, value]),
  );
  const ratio = (first, second) => {
    const luminance = color => [1, 3, 5].map(start => parseInt(color.slice(start, start + 2), 16) / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
      .reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
    const a = luminance(first), b = luminance(second);
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  };
  const base = variables(":root");
  const dark = {...base, ...variables(':root\\[data-theme="dark"\\]')};
  const embed = {...base, ...variables(':root\\[data-palette="embed"\\]')};
  const embedDark = {...dark, ...variables(':root\\[data-palette="embed"\\]'), ...variables(':root\\[data-palette="embed"\\]\\[data-theme="dark"\\]')};
  for (const [name, colors] of [["light",base],["dark",dark],["embed light",embed],["embed dark",embedDark]]) {
    for (const surface of ["disabled-surface", "disabled-primary"]) {
      assert(ratio(colors["disabled-ink"], colors[surface]) >= 4.5, `${name} disabled text on ${surface} is unreadable`);
    }
  }
  assert(ratio(base["indigo-ink"], base["indigo-hover"]) >= 4.5, "Indigo key hover text is unreadable");
  assert(ratio(base["key-ink"], base["key-hover"]) >= 4.5, "Key hover text is unreadable");
  assert.match(style, /input::placeholder, textarea::placeholder \{ color: var\(--muted\); opacity: 1; \}/);
  assert.doesNotMatch(style.match(/\.key-binding \{[^}]+\}/)?.[0] ?? "", /opacity:/);
});
