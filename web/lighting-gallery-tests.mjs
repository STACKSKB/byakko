// SPDX-License-Identifier: GPL-3.0-or-later
// Browser integration checks use the real WASM session and the synthetic Nia87.
import * as codec from "./pkg/byakko_web.js";
function assert(value, message) { if (!value) throw new Error(message); }
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
async function until(predicate) {
  const end = performance.now() + 5000;
  while (!predicate()) { assert(performance.now() < end, "Timed out waiting for lighting state"); await tick(); }
}
function button(root, label) {
  const found = [...root.querySelectorAll("button")].find(item => item.textContent === label);
  assert(found && !found.disabled, `Missing enabled button: ${label}`);
  return found;
}
async function ready(h) {
  button(h.root, "Lighting").click();
  const field = [...h.root.querySelectorAll("label.field")].find(item => item.querySelector("span")?.textContent === "Lighting mode");
  assert(field, "Lighting mode control missing");
  const selector = field.querySelector("select");
  selector.value = "onboard:13";
  selector.dispatchEvent(new Event("change", {bubbles: true}));
  await until(() => h.app.view.lighting.editor.draft?.effect === "13" && !h.app.view.lighting.editor.dirty && !h.app.view.busy);
  button(h.root, "Load selected layer").click();
  await until(() => h.app.view.picture.editor.canEdit && h.app.view.picture.editor.draft);
  assert(h.root.querySelectorAll(".lux-tile").length === 8, "Eight look tiles were not rendered");
}
function key(h, label) { return h.app.view.descriptor.keys.find(item => item.label === label && item.visible)?.id; }
function boardColor(h, id) { return h.root.querySelector(`.keyboard .key[data-key="${id}"]`)?.style.getPropertyValue("--key-color"); }

export const galleryTests = [
  ["brush selection does not paint until Paint and a key click", async h => {
    await ready(h);
    const w = key(h, "W"), before = h.app.view.picture.editor.draft[w].join();
    const writes = h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x0c).length;
    const picker = [...h.root.querySelectorAll("label.field")].find(item => item.querySelector("span")?.textContent === "Brush color")?.querySelector("input[type=color]");
    assert(picker, "Brush picker missing");
    picker.value = "#123456";
    picker.dispatchEvent(new Event("change", {bubbles: true}));
    assert(h.app.view.picture.editor.draft[w].join() === before, "Selecting a brush edited a key");
    assert(h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x0c).length === writes,
      "Selecting a brush sent a picture write");
    button(h.root, "Paint").click();
    h.root.querySelector(`.keyboard .key[data-key="${w}"]`).click();
    await until(() => h.app.view.picture.editor.draft[w]?.join() === "18,52,86");
    button(h.root, "WASD").click();
    assert(boardColor(h, w) === "#123456", "Preview lost accepted hand paint");
    button(h.root, "Revert preview").click();
    button(h.root, "Discard changes").click();
    await until(() => !h.app.view.picture.editor.dirty && !h.app.view.busy);
    const original = h.app.view.lighting.editor.draft.option;
    const other = original === "1" ? "2" : "1";
    const switched = await h.app.intent({type: "edit", feature: "lighting", change: {Option: other}});
    assert(switched.ok, "Could not select another stored layer");
    const switchSave = await h.app.intent({type: "apply", feature: "lighting"});
    assert(switchSave.ok, "Could not save another stored layer selection");
    await until(() => !h.app.view.lighting.editor.dirty && !h.app.view.busy);
    button(h.root, "Load selected layer").click();
    await until(() => h.app.view.picture.editor.canEdit &&
      h.app.view.lighting.editor.draft.option === other);
    button(h.root, "WASD").click();
    assert(boardColor(h, w) !== "#123456", "Paint from another layer leaked into the preview");
    button(h.root, "Revert preview").click();
    button(h.root, "Discard changes").click();
    const returned = await h.app.intent({type: "edit", feature: "lighting", change: {Option: original}});
    assert(returned.ok, "Could not return to the painted layer");
    const returnSave = await h.app.intent({type: "apply", feature: "lighting"});
    assert(returnSave.ok, "Could not save the returned layer selection");
    await until(() => !h.app.view.lighting.editor.dirty && !h.app.view.busy);
    button(h.root, "Load selected layer").click();
    await until(() => h.app.view.picture.editor.canEdit &&
      h.app.view.lighting.editor.draft.option === original);
    button(h.root, "WASD").click();
    assert(boardColor(h, w) === "#123456", "Returning to a painted layer lost its provenance");
  }],
  ["look preview keeps pending paint and writes only on Apply", async h => {
    await ready(h);
    const w = key(h, "W"); assert(w, "W position missing");
    const pending = [9, 8, 7];
    const edited = await h.app.intent({type: "edit", feature: "picture", change: {Color: {key: w, color: pending}}});
    assert(edited.ok && h.app.view.picture.editor.dirty, "Pending paint was not staged");
    const writes = h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x0c).length;
    const lightingWrites = h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x07).length;
    button(h.root, "WASD").click();
    assert(boardColor(h, w) === "#090807", "Look did not preserve the pending painted key");
    assert(h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x0c).length === writes,
      "Preview sent a picture write");
    const replace = h.root.querySelector(".lux-look-options input[type=checkbox]");
    assert(replace, "Replace painted keys control missing");
    replace.checked = true; replace.dispatchEvent(new Event("change", {bubbles: true}));
    assert(boardColor(h, w) === "#ff3c00", "Explicit Replace did not update the local preview");
    assert(h.app.view.picture.editor.draft[w].join() === pending.join(), "Preview edited the core draft");
    button(h.root, "Revert preview").click();
    button(h.root, "Discard changes").click();
    await until(() => !h.root.querySelector(".lux-look-options"));
    assert(h.app.view.picture.editor.draft[w].join() === pending.join(), "Discard lost ordinary pending paint");
    assert(h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x0c).length === writes,
      "Discard sent a picture write");
    button(h.root, "WASD").click();
    button(h.root, "Apply look").click();
    await until(() => !h.app.view.picture.editor.dirty &&
      h.device.calls.some(call => call.kind === "send" && call.report[0] === 0x0c));
    assert(h.app.view.picture.editor.baseline.content.Editable[w].join() === pending.join(),
      "Apply overwrote pending paint without Replace");
    assert(!h.root.querySelector(".lux-look-options"), "Applied look remained staged");
    assert(h.device.calls.filter(call => call.kind === "send" && call.report[0] === 0x07).length === lightingWrites,
      "Applying a look changed global lighting settings");
  }],
  ["rejected look write restores the prior draft before later paint", async h => {
    await ready(h);
    const w = key(h, "W"), a = key(h, "A");
    const before = [9, 8, 7];
    const otherBefore = h.app.view.picture.editor.draft[a].join();
    assert((await h.app.intent({type: "edit", feature: "picture", change: {Color: {key: w, color: before}}})).ok,
      "Could not stage ordinary paint");
    button(h.root, "WASD").click();
    const prototype = codec.BrowserSession.prototype;
    const original = prototype.dispatch;
    let rejected = false;
    prototype.dispatch = function(input) {
      const intent = JSON.parse(input);
      if (!rejected && intent.type === "apply" && intent.feature === "picture") {
        rejected = true;
        return JSON.stringify({ok: false, error: "Synthetic write-start rejection", view: h.app.view});
      }
      return original.call(this, input);
    };
    try {
      button(h.root, "Apply look").click();
      await until(() => rejected && !h.root.querySelector(".lux-actions button.primary")?.disabled);
      assert(h.app.view.picture.editor.draft[w].join() === before.join(), "Rejected look left its staged color in the draft");
      assert(h.app.view.picture.editor.draft[a].join() === otherBefore, "Rejected look left another preset color in the draft");
    } finally { prototype.dispatch = original; }
    button(h.root, "Revert preview").click();
    button(h.root, "Discard changes").click();
    await until(() => !h.root.querySelector(".lux-actions"));
    button(h.root, "Paint").click();
    const picker = [...h.root.querySelectorAll("label.field")].find(item => item.querySelector("span")?.textContent === "Brush color")?.querySelector("input[type=color]");
    picker.value = "#345678"; picker.dispatchEvent(new Event("change", {bubbles: true}));
    h.root.querySelector(`.keyboard .key[data-key="${w}"]`).click();
    await until(() => h.app.view.picture.editor.baseline.content.Editable[w]?.join() === "52,86,120");
    assert(h.app.view.picture.editor.baseline.content.Editable[a].join() === otherBefore,
      "Later manual paint accidentally autosaved rejected look colors");
  }],
];
