// SPDX-License-Identifier: GPL-3.0-or-later
import { boardBounds, keyRectangle } from "../key-geometry.mjs";
import { usageForCode } from "../input.mjs";
import { defaultLayout, layoutFor } from "./layouts.mjs";
import { emptyTest, updateTest, coverage } from "./state.mjs";
import { copy } from "./copy.mjs";
import { connectPageInput } from "./page-input.mjs";

export function mountTester(root) {
  if (root.dataset.mounted) return;
  root.dataset.mounted = "true";
  const doc = root.ownerDocument;
  const surface = root.querySelector("[data-surface]");
  const board = root.querySelector("[data-board]");
  const status = root.querySelector("[data-status]");
  const choices = { ...defaultLayout };
  let keys = [];
  let elements = new Map();
  let state = emptyTest();

  function renderState() {
    for (const [code, element] of elements) {
      const held = state.held.has(code);
      const tested = state.tested.has(code);
      element.classList.toggle("bt-held", held);
      element.classList.toggle("bt-tested", tested);
      const item = keys.find(key => key.code === code);
      element.setAttribute("aria-label", `${item.label} · ${code}${held ? ` · ${copy.held}` : tested ? ` · ${copy.tested}` : ""}`);
    }
    const count = coverage(state, keys);
    root.querySelector("[data-tested]").textContent = `${count.tested} / ${count.total}`;
    root.querySelector("[data-held-count]").textContent = String(state.held.size);
    root.querySelector("[data-peak]").textContent = String(state.peak);
    root.querySelector("[data-held]").textContent = [...state.held].join(" + ") || copy.none;
    const last = state.last;
    const details = root.querySelector("[data-event]");
    details.hidden = !last;
    if (!last) details.textContent = "";
    else {
      const usage = usageForCode(last.code);
      details.textContent = `${last.pressed ? copy.down : copy.up} · code: ${last.code || copy.unavailable} · key: ${JSON.stringify(last.key)} · ${copy.locations[last.location] ?? last.location}${last.repeat ? ` · ${copy.repeating}` : ""}${usage !== null ? ` · USB usage: 0x${usage.toString(16).padStart(2, "0")}` : ""}${!elements.has(last.code) ? ` · ${copy.outside}` : ""}`;
    }
  }

  function renderLayout() {
    keys = layoutFor(choices);
    const bounds = boardBounds(keys);
    board.style.width = `${bounds.width * 43 + 14}px`;
    board.style.height = `${bounds.height * 43 + 14}px`;
    elements = new Map();
    board.replaceChildren();
    for (const item of keys) {
      const element = doc.createElement("span");
      element.className = `bt-key${item.code ? "" : " bt-unavailable"}`;
      element.textContent = item.label;
      element.dataset.code = item.code ?? "";
      if (item.shape) element.dataset.shape = item.shape;
      element.title = item.code ?? copy.fn;
      element.setAttribute("aria-label", item.code ? `${item.label} · ${item.code}` : copy.fn);
      for (const [property, value] of Object.entries(keyRectangle(item))) element.style[property] = `${value}px`;
      board.append(element);
      if (item.code) elements.set(item.code, element);
    }
    renderState();
  }

  function apply(event) {
    state = updateTest(state, event);
    renderState();
  }
  function pause(reason) {
    apply({ type: "release-all" });
    status.textContent = reason === "navigation" ? copy.navigation : copy.paused;
  }
  for (const select of root.querySelectorAll("[data-choice]")) {
    select.value = choices[select.dataset.choice];
    select.addEventListener("change", () => {
      choices[select.dataset.choice] = select.value;
      if (select.dataset.choice === "standard" && select.value === "jis") {
        choices.legends = "japanese";
        root.querySelector('[data-choice="legends"]').value = "japanese";
      } else if (select.dataset.choice === "standard" && choices.legends === "japanese") {
        choices.legends = "qwerty";
        root.querySelector('[data-choice="legends"]').value = "qwerty";
      }
      // Keep tested history across layout changes; only coverage is reprojected.
      state = updateTest(state, { type: "release-all" });
      renderLayout();
    });
  }
  renderLayout();
  const activate = connectPageInput(root, { input: apply, pause,
    resume: () => { status.textContent = copy.active; } });
  root.querySelector('[data-action="reset"]').addEventListener("click", () => {
    apply({ type: "reset" });
    surface.focus({ preventScroll: true });
    activate();
  });
  surface.addEventListener("pointerdown", () => {
    surface.focus({ preventScroll: true });
    activate();
  });
  surface.addEventListener("contextmenu", event => event.preventDefault());
}

for (const root of document.querySelectorAll("[data-byakko-tester]")) mountTester(root);
