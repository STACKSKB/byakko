// SPDX-License-Identifier: GPL-3.0-or-later
// Run after `npm run build:tester`, with CHROMIUM_BIN=/usr/bin/chromium.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const webRoot = dirname(fileURLToPath(import.meta.url));
const artifact = resolve(webRoot, "dist/tester/index.html");
const browserBinary = process.env.CHROMIUM_BIN;
const browserOptions = { timeout: 30_000, skip: !browserBinary && "Set CHROMIUM_BIN to run browser QA" };
const screenshotDirectory = process.env.BYAKKO_TESTER_SCREENSHOT_DIR;

async function waitFor(fn) {
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      const result = await fn();
      if (result) return result;
    } catch { /* Browser initialization may still be in progress. */ }
    await new Promise(done => setTimeout(done, 50));
  }
  throw new Error("Chromium DevTools did not start");
}

async function launchBrowser() {
  const directory = await mkdtemp(resolve(tmpdir(), "byakko-tester-cdp-"));
  const child = spawn(browserBinary, [
    "--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage",
    `--user-data-dir=${directory}`, "--remote-debugging-port=0", "about:blank",
  ], { stdio: "ignore", env: { ...process.env, XDG_CONFIG_HOME: directory } });
  const port = await waitFor(async () => {
    if (child.exitCode !== null) throw new Error(`Chromium exited: ${child.exitCode}`);
    const content = await readFile(resolve(directory, "DevToolsActivePort"), "utf8");
    return Number(content.split("\n")[0]);
  });
  const targets = await waitFor(async () => {
    const response = await fetch(`http://127.0.0.1:${port}/json/list`);
    return (await response.json()).filter(target => target.type === "page");
  });
  const socket = new WebSocket(targets[0].webSocketDebuggerUrl);
  await new Promise((done, fail) => {
    socket.addEventListener("open", done, { once: true });
    socket.addEventListener("error", fail, { once: true });
  });
  let nextId = 0;
  const pending = new Map();
  socket.addEventListener("message", event => {
    const message = JSON.parse(event.data);
    if (!message.id) return;
    const request = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error.message));
    else request.resolve(message.result);
  });
  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async function close() {
    socket.close();
    child.kill();
    if (child.exitCode === null) await new Promise(done => child.once("exit", done));
  }
  return { send, close };
}

test("standalone tester starts automatically and captures browser input across layouts", browserOptions, async () => {
  await readFile(artifact, "utf8").catch(() => {
    throw new Error("Build the browser tester first: npm --prefix web run build:tester");
  });
  const browser = await launchBrowser();
  const { send } = browser;
  const evaluate = async expression => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  };
  const down = (code, key, virtualKeyCode, location = 0, autoRepeat = false) => send("Input.dispatchKeyEvent", {
    type: "rawKeyDown", code, key, windowsVirtualKeyCode: virtualKeyCode,
    nativeVirtualKeyCode: virtualKeyCode, location, isKeypad: location === 3, autoRepeat,
  });
  const up = (code, key, virtualKeyCode, location = 0) => send("Input.dispatchKeyEvent", {
    type: "keyUp", code, key, windowsVirtualKeyCode: virtualKeyCode,
    nativeVirtualKeyCode: virtualKeyCode, location, isKeypad: location === 3,
  });
  const screenshot = async name => {
    if (!screenshotDirectory) return;
    await mkdir(screenshotDirectory, { recursive: true });
    const capture = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
    await writeFile(resolve(screenshotDirectory, `byakko-tester-${name}.png`), Buffer.from(capture.data, "base64"));
  };
  try {
    await send("Page.enable");
    await send("Runtime.enable");
    await send("Page.navigate", { url: pathToFileURL(artifact).href });
    await waitFor(() => evaluate("document.querySelector('[data-byakko-tester]')?.dataset.mounted === 'true'"));
    assert.equal(await evaluate("document.querySelectorAll('[data-board] .bt-key').length > 100"), true);
    assert.equal(await evaluate("performance.getEntriesByType('resource').length"), 0);
    const firstTotal = await evaluate("document.querySelector('[data-tested]').textContent");
    assert.match(firstTotal, /^0 \/ \d+$/);
    assert.equal(await evaluate("document.querySelector('[data-action=start]')"), null);
    assert.equal(await evaluate("document.querySelector('.bt-heading').contains(document.querySelector('[data-action=reset]'))"), true);
    const desktopHeader = await evaluate("(() => { const h=document.querySelector('.bt-heading').getBoundingClientRect(), r=document.querySelector('[data-action=reset]').getBoundingClientRect(); return {headerRight:h.right, resetRight:r.right, resetLeft:r.left, headerMid:h.left+h.width/2}; })()");
    assert.ok(Math.abs(desktopHeader.headerRight - desktopHeader.resetRight) <= 3, JSON.stringify(desktopHeader));
    assert.ok(desktopHeader.resetLeft > desktopHeader.headerMid, JSON.stringify(desktopHeader));

    // The first key is counted without clicking or focusing the board.
    await down("KeyA", "a", 65);
    assert.equal(await evaluate("document.querySelector('[data-code=KeyA]').classList.contains('bt-held')"), true);
    await down("KeyA", "a", 65, 0, true);
    assert.equal(await evaluate("document.querySelector('[data-held-count]').textContent"), "1");
    assert.equal(await evaluate("document.querySelector('[data-event]').textContent.includes('repeating')"), true);
    await down("ShiftLeft", "Shift", 16, 1);
    await down("ShiftRight", "Shift", 16, 2);
    assert.equal(await evaluate("document.querySelector('[data-held-count]').textContent"), "3");
    assert.equal(await evaluate("document.querySelector('[data-peak]').textContent"), "3");
    await up("KeyA", "a", 65); await up("ShiftLeft", "Shift", 16, 1);
    await up("ShiftRight", "Shift", 16, 2);
    assert.equal(await evaluate("document.querySelector('[data-code=KeyA]').classList.contains('bt-tested')"), true);
    assert.equal(await evaluate("document.querySelector('[data-held-count]').textContent"), "0");
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('3 /')"), true);

    // Numpad and main row are distinct, even when the character is the same.
    await down("Digit1", "1", 49); await up("Digit1", "1", 49);
    await down("Numpad1", "1", 97, 3); await up("Numpad1", "1", 97, 3);
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('5 /')"), true);

    // Switching the pictured layout reprojects coverage while retaining history.
    await evaluate("document.querySelector('[data-choice=size]').value='60'; document.querySelector('[data-choice=size]').dispatchEvent(new Event('change'))");
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('4 /')"), true);
    await down("Numpad2", "2", 98, 3); await up("Numpad2", "2", 98, 3);
    assert.equal(await evaluate("document.querySelector('[data-event]').textContent.includes('Outside selected layout')"), true);
    await evaluate("document.querySelector('[data-choice=size]').value='full'; document.querySelector('[data-choice=size]').dispatchEvent(new Event('change'))");
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('6 /')"), true);
    await evaluate("document.querySelector('[data-choice=standard]').value='jis'; document.querySelector('[data-choice=standard]').dispatchEvent(new Event('change'))");
    assert.equal(await evaluate("document.querySelector('[data-choice=legends]').value"), "japanese");
    assert.equal(await evaluate("!!document.querySelector('[data-code=IntlRo]')"), true);
    assert.equal(await evaluate("document.querySelector('[data-code=Backspace]').textContent"), "Bksp");
    await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
    await screenshot("jis");
    await evaluate("document.querySelector('[data-choice=standard]').value='iso'; document.querySelector('[data-choice=standard]').dispatchEvent(new Event('change'))");
    assert.equal(await evaluate("document.querySelector('[data-choice=legends]').value"), "qwerty");
    await screenshot("iso");

    // Native control keys still work. Tab remains normal page navigation.
    await evaluate("document.querySelector('[data-choice=size]').focus()");
    const priorSize = await evaluate("document.querySelector('[data-choice=size]').value");
    await down("ArrowDown", "ArrowDown", 40); await up("ArrowDown", "ArrowDown", 40);
    assert.notEqual(await evaluate("document.querySelector('[data-choice=size]').value"), priorSize);
    await evaluate("document.querySelector('[data-surface]').focus()");
    await down("Tab", "Tab", 9); await up("Tab", "Tab", 9);
    assert.equal(await evaluate("document.activeElement.matches('[data-surface]')"), false);

    // An editable field outside the tester receives text without changing its history.
    const beforeExternal = await evaluate("document.querySelector('[data-tested]').textContent");
    await evaluate("(() => { const input=document.createElement('input'); input.id='outside-tester'; document.body.append(input); input.focus(); })()");
    await send("Input.dispatchKeyEvent", { type: "keyDown", code: "KeyZ", key: "z", text: "z", unmodifiedText: "z", windowsVirtualKeyCode: 90 });
    await up("KeyZ", "z", 90);
    assert.equal(await evaluate("document.querySelector('#outside-tester').value"), "z");
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent"), beforeExternal);

    // Reset moves focus to the board, ready for another key without a start action.
    await evaluate("document.querySelector('[data-action=reset]').click()");
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('0 /')"), true);
    assert.equal(await evaluate("document.querySelector('[data-peak]').textContent"), "0");
    assert.equal(await evaluate("document.activeElement.matches('[data-surface]')"), true);
    await down("KeyB", "b", 66); await up("KeyB", "b", 66);
    assert.equal(await evaluate("document.querySelector('[data-tested]').textContent.startsWith('1 /')"), true);

    // Focus and visibility loss must clear apparent held keys.
    await down("KeyC", "c", 67);
    await evaluate("window.dispatchEvent(new Event('blur'))");
    assert.equal(await evaluate("document.querySelector('[data-held-count]').textContent"), "0");
    await down("KeyD", "d", 68);
    await evaluate("Object.defineProperty(document, 'hidden', { configurable:true, get:()=>true }); document.dispatchEvent(new Event('visibilitychange'))");
    assert.equal(await evaluate("document.querySelector('[data-held-count]').textContent"), "0");
    assert.equal(await evaluate("document.querySelector('[data-code=KeyB]').classList.contains('bt-tested')"), true);
  } finally {
    await browser.close();
  }
});

test("two shortcode roots are isolated and the mobile page does not overflow", browserOptions, async () => {
  const html = await readFile(artifact, "utf8");
  const section = html.match(/<section class="byakko-tester"[\s\S]*?<\/section>/)?.[0];
  assert.ok(section);
  const directory = await mkdtemp(resolve(tmpdir(), "byakko-tester-pages-"));
  const page = resolve(directory, "two.html");
  await writeFile(page, html.replace(section, section + section));
  const browser = await launchBrowser();
  const { send } = browser;
  const evaluate = async expression => (await send("Runtime.evaluate", { expression, returnByValue: true })).result.value;
  const press = async (code, key, virtualKeyCode) => {
    await send("Input.dispatchKeyEvent", { type: "rawKeyDown", code, key, windowsVirtualKeyCode: virtualKeyCode });
    await send("Input.dispatchKeyEvent", { type: "keyUp", code, key, windowsVirtualKeyCode: virtualKeyCode });
  };
  try {
    await send("Page.enable"); await send("Runtime.enable");
    await send("Page.navigate", { url: pathToFileURL(page).href });
    await waitFor(() => evaluate("document.querySelectorAll('[data-mounted=true]').length === 2"));
    await press("KeyA", "a", 65);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[0].textContent.startsWith('1 /')"), true);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[1].textContent.startsWith('0 /')"), true);
    await evaluate("document.querySelectorAll('[data-surface]')[1].dispatchEvent(new PointerEvent('pointerdown', { bubbles:true }))");
    await press("KeyB", "b", 66);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[0].textContent.startsWith('1 /')"), true);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[1].textContent.startsWith('1 /')"), true);
    await evaluate("document.querySelectorAll('[data-choice=standard]')[0].focus()");
    await press("KeyC", "c", 67);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[0].textContent.startsWith('2 /')"), true);
    assert.equal(await evaluate("document.querySelectorAll('[data-tested]')[1].textContent.startsWith('1 /')"), true);
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    const dimensions = await evaluate("({body:document.body.scrollWidth, viewport:innerWidth, scroller:document.querySelector('[data-surface]').scrollWidth})");
    assert.ok(dimensions.body <= dimensions.viewport, JSON.stringify(dimensions));
    assert.ok(dimensions.scroller > dimensions.viewport);
    const mobileHeader = await evaluate("(() => { const h=document.querySelector('.bt-heading').getBoundingClientRect(), r=document.querySelector('[data-action=reset]').getBoundingClientRect(); return {headerRight:h.right, resetRight:r.right, resetLeft:r.left, headerMid:h.left+h.width/2}; })()");
    assert.ok(Math.abs(mobileHeader.headerRight - mobileHeader.resetRight) <= 3, JSON.stringify(mobileHeader));
    assert.ok(mobileHeader.resetLeft > mobileHeader.headerMid, JSON.stringify(mobileHeader));
    if (screenshotDirectory) {
      await mkdir(screenshotDirectory, { recursive: true });
      const capture = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
      await writeFile(resolve(screenshotDirectory, "byakko-tester-mobile.png"), Buffer.from(capture.data, "base64"));
    }
  } finally {
    await browser.close();
  }
});
