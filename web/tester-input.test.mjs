// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { connectPageInput } from "./tester/page-input.mjs";

function fixture() {
  const listeners = new Map();
  const windowListeners = new Map();
  const document = {
    hidden: false,
    activeElement: null,
    hasFocus: () => true,
    addEventListener(name, listener) {
      const group = listeners.get(name) ?? [];
      group.push(listener);
      listeners.set(name, group);
    },
    defaultView: {
      addEventListener(name, listener) {
        const group = windowListeners.get(name) ?? [];
        group.push(listener);
        windowListeners.set(name, group);
      },
    },
  };
  function dispatch(group, name, target, attributes = {}) {
    const event = {
      target, code: "", key: "", location: 0, repeat: false, shiftKey: false,
      defaultPrevented: false, propagationStopped: false, ...attributes,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.propagationStopped = true; },
    };
    for (const listener of group.get(name) ?? []) listener(event);
    return event;
  }
  function element(parent = null, kind = "plain") {
    const self = {
      ownerDocument: document, parent, kind,
      contains(other) {
        for (let item = other; item; item = item.parent) if (item === this) return true;
        return false;
      },
      closest(selector) {
        for (let item = this; item; item = item.parent) {
          if (selector === "[data-byakko-tester]" && item.kind === "root") return item;
          if (selector === "[data-surface]" && item.kind === "surface") return item;
          if (selector.includes("input, textarea") && ["input", "editable"].includes(item.kind)) return item;
          if (selector.includes("select, button") && ["select", "button", "link", "summary"].includes(item.kind)) return item;
        }
        return null;
      },
    };
    return self;
  }
  const body = element();
  const outsideButton = element(body, "button");
  const outsideInput = element(body, "input");
  function tester() {
    const root = element(body, "root");
    const surface = element(root, "surface");
    const reset = element(root, "button");
    root.querySelector = selector => selector === "[data-surface]" ? surface :
      selector === '[data-action="reset"]' ? reset : null;
    return { root, surface, reset, select: element(root, "select"),
      button: element(root, "button"), input: element(root, "input") };
  }
  function handlers() {
    const events = [];
    const pauses = [];
    let resumes = 0;
    return { events, pauses, get resumes() { return resumes; },
      input: event => events.push(event), pause: reason => pauses.push(reason),
      resume: () => { resumes++; } };
  }
  function focus(target) {
    document.activeElement = target;
    dispatch(listeners, "focusin", target);
  }
  function key(name, target, code, attributes = {}) {
    return dispatch(listeners, name, target, { code, key: code, ...attributes });
  }
  return { document, body, outsideButton, outsideInput, tester, handlers, focus, key,
    pointer: target => dispatch(listeners, "pointerdown", target),
    visibility: () => dispatch(listeners, "visibilitychange", document),
    windowEvent: name => dispatch(windowListeners, name, document.defaultView) };
}

test("page capture records plain Tab, including focused tester controls, while outside controls keep normal input", () => {
  const page = fixture();
  const one = page.tester();
  const owner = page.handlers();
  const activate = connectPageInput(one.root, owner);
  assert.equal(typeof activate, "function");

  const first = page.key("keydown", page.body, "Tab");
  assert.equal(first.defaultPrevented, true);
  assert.equal(owner.events.at(-1).code, "Tab");
  assert.equal(owner.events.at(-1).pressed, true);
  page.key("keyup", page.body, "Tab");
  assert.equal(owner.events.at(-1).pressed, false);

  page.focus(one.select);
  const inSelect = page.key("keydown", one.select, "Tab");
  assert.equal(inSelect.defaultPrevented, true);
  assert.equal(owner.events.at(-1).code, "Tab");
  const arrow = page.key("keydown", one.select, "ArrowDown");
  assert.equal(arrow.defaultPrevented, false);

  const before = owner.events.length;
  page.focus(page.outsideInput);
  assert.equal(page.key("keydown", page.outsideInput, "KeyA").defaultPrevented, false);
  page.focus(page.outsideButton);
  assert.equal(page.key("keydown", page.outsideButton, "Tab").defaultPrevented, false);
  assert.equal(owner.events.length, before);
});

test("Shift+Tab escapes into persistent navigation and releases held keys", () => {
  const page = fixture();
  const one = page.tester();
  const owner = page.handlers();
  connectPageInput(one.root, owner);
  page.focus(one.surface);
  page.key("keydown", one.surface, "ShiftLeft", { key: "Shift", shiftKey: true });
  const before = owner.events.length;
  const escape = page.key("keydown", one.surface, "Tab", { shiftKey: true });
  assert.equal(escape.defaultPrevented, false);
  assert.equal(owner.events.length, before, "escape Tab is not recorded");
  assert.ok(owner.pauses.includes("navigation"), "navigation releases held inputs through the owner");
  page.key("keyup", one.surface, "Tab", { shiftKey: true });
  assert.equal(owner.events.length, before, "escape keyup is also ignored");
  const resumes = owner.resumes;

  page.focus(one.select);
  assert.equal(page.key("keydown", one.select, "Tab").defaultPrevented, false);
  page.focus(page.outsideButton);
  page.key("keydown", page.outsideButton, "KeyB");
  page.windowEvent("blur");
  page.windowEvent("focus");
  page.document.hidden = true;
  page.visibility();
  page.document.hidden = false;
  page.visibility();
  page.focus(one.surface);
  assert.equal(page.key("keydown", one.surface, "KeyC").defaultPrevented, false);
  assert.equal(owner.events.length, before, "focus and visibility do not silently restart capture");
  assert.equal(owner.resumes, resumes, "navigation never reports listening during focus changes");
});

test("board activation and reset rearm; Enter or Space on the board are consumed as activation", () => {
  const page = fixture();
  const one = page.tester();
  const owner = page.handlers();
  const activate = connectPageInput(one.root, owner);
  page.focus(one.surface);
  page.key("keydown", one.surface, "Tab", { shiftKey: true });
  page.key("keyup", one.surface, "Tab", { shiftKey: true });

  const before = owner.events.length;
  const enter = page.key("keydown", one.surface, "Enter");
  assert.equal(enter.defaultPrevented, true);
  assert.equal(page.key("keydown", one.surface, "Enter", { repeat: true }).defaultPrevented, true);
  assert.equal(page.key("keyup", one.surface, "Enter").defaultPrevented, true);
  assert.equal(owner.events.length, before, "activation key is not tested");
  assert.equal(page.key("keydown", one.surface, "KeyA").defaultPrevented, true);
  assert.equal(owner.events.at(-1).code, "KeyA");

  page.key("keydown", one.surface, "Tab", { shiftKey: true });
  page.key("keyup", one.surface, "Tab", { shiftKey: true });
  const beforeSpace = owner.events.length;
  assert.equal(page.key("keydown", one.surface, "Space", { key: " " }).defaultPrevented, true);
  assert.equal(page.key("keyup", one.surface, "Space", { key: " " }).defaultPrevented, true);
  assert.equal(owner.events.length, beforeSpace);
  assert.equal(page.key("keydown", one.surface, "KeyB").defaultPrevented, true);

  page.key("keydown", one.surface, "Tab", { shiftKey: true });
  page.key("keyup", one.surface, "Tab", { shiftKey: true });
  page.focus(one.surface);
  activate(); // Board click or Reset calls this after focusing the surface.
  assert.equal(page.key("keydown", one.surface, "KeyC").defaultPrevented, true);
  assert.equal(owner.events.at(-1).code, "KeyC");
});

test("another tester can be selected during navigation without rearming either one", () => {
  const page = fixture();
  const first = page.tester();
  const second = page.tester();
  const firstOwner = page.handlers();
  const secondOwner = page.handlers();
  connectPageInput(first.root, firstOwner);
  const activateSecond = connectPageInput(second.root, secondOwner);
  page.focus(first.surface);
  page.key("keydown", first.surface, "Tab", { shiftKey: true });
  page.key("keyup", first.surface, "Tab", { shiftKey: true });

  page.pointer(second.select);
  page.focus(second.select);
  const priorFirst = firstOwner.events.length;
  const priorSecond = secondOwner.events.length;
  assert.equal(page.key("keydown", second.select, "Tab").defaultPrevented, false);
  page.focus(second.surface);
  assert.equal(page.key("keydown", second.surface, "KeyA").defaultPrevented, false);
  assert.equal(firstOwner.events.length, priorFirst);
  assert.equal(secondOwner.events.length, priorSecond);

  activateSecond();
  assert.equal(page.key("keydown", second.surface, "KeyB").defaultPrevented, true);
  assert.equal(secondOwner.events.at(-1).code, "KeyB");
  assert.equal(firstOwner.events.length, priorFirst);
});

test("Enter on Reset navigates through native click, then its keyup is not a new test sample", () => {
  const page = fixture();
  const one = page.tester();
  const owner = page.handlers();
  const activate = connectPageInput(one.root, owner);
  page.focus(one.surface);
  page.key("keydown", one.surface, "Tab", { shiftKey: true });
  page.key("keyup", one.surface, "Tab", { shiftKey: true });

  page.focus(one.reset);
  const before = owner.events.length;
  const down = page.key("keydown", one.reset, "Enter");
  assert.equal(down.defaultPrevented, false, "the native Enter click must still fire");
  assert.equal(owner.events.length, before, "navigation does not record Reset's keydown");
  // The native click handler resets the test, focuses the board, then activates it.
  page.focus(one.surface);
  activate();
  const up = page.key("keyup", one.surface, "Enter");
  assert.equal(up.defaultPrevented, true);
  assert.equal(owner.events.length, before, "Reset's trailing keyup is not recorded after capture resumes");
  assert.equal(page.key("keydown", one.surface, "KeyD").defaultPrevented, true);
  assert.equal(owner.events.at(-1).code, "KeyD");
});
