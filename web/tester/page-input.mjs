// SPDX-License-Identifier: GPL-3.0-or-later
// One active tester per page, with ordinary form controls retaining their input.
const pages = new WeakMap();
const controlSelector = 'input, textarea, select, button, a, summary, [contenteditable]:not([contenteditable="false"]), [role="textbox"]';
const editingSelector = 'input, textarea, [contenteditable]:not([contenteditable="false"]), [role="textbox"]';

export function connectPageInput(root, handlers) {
  const doc = root.ownerDocument;
  let page = pages.get(doc);
  if (!page) {
    page = { testers: new Map(), active: root, captureEnabled: true, resumeKey: null };
    pages.set(doc, page);

    function select(target) {
      const nearest = target?.closest?.("[data-byakko-tester]");
      if (page.testers.has(nearest) && nearest !== page.active) {
        page.testers.get(page.active)?.pause();
        page.active = nearest;
      }
      const control = target?.closest?.(controlSelector);
      const editing = target?.closest?.(editingSelector);
      const ignored = editing || (control && !page.active.contains(control));
      return { handlers: page.testers.get(page.active), control, ignored };
    }

    function sync(target = doc.activeElement) {
      const selected = select(target);
      if (!page.captureEnabled) selected.handlers?.pause("navigation");
      else if (selected.ignored || doc.hidden || !doc.hasFocus()) selected.handlers?.pause();
      else selected.handlers?.resume();
    }
    page.sync = sync;

    doc.addEventListener("pointerdown", event => sync(event.target));
    doc.addEventListener("focusin", event => sync(event.target));
    for (const [name, pressed] of [["keydown", true], ["keyup", false]]) {
      doc.addEventListener(name, event => {
        const selected = select(event.target);
        const code = event.code || event.key;
        if (code === page.resumeKey) {
          // The key used to resume is an activation gesture, not a test sample.
          if (!pressed) page.resumeKey = null;
          event.preventDefault();
          event.stopPropagation();
          return;
        }
        if (!selected.handlers || doc.hidden || selected.ignored) return;
        // Enter clicks Reset on keydown. Ignore its later keyup after Reset rearms.
        if (pressed && event.key === "Enter" && selected.control &&
            selected.control === page.active.querySelector('[data-action="reset"]')) page.resumeKey = code;
        const tab = event.code === "Tab" || event.key === "Tab";
        if (page.captureEnabled && pressed && tab && event.shiftKey &&
            !event.ctrlKey && !event.altKey && !event.metaKey) {
          page.captureEnabled = false;
          selected.handlers.pause("navigation");
          // Native Shift+Tab moves focus back; subsequent Tab navigation stays free.
          return;
        }
        if (!page.captureEnabled) {
          const surface = page.active.querySelector("[data-surface]");
          if (!pressed || event.target !== surface || (event.key !== "Enter" && event.key !== " ")) return;
          page.captureEnabled = true;
          page.resumeKey = code;
          event.preventDefault();
          event.stopPropagation();
          selected.handlers.resume();
          return;
        }
        // Tab is a test key during capture; other native control actions still work.
        if (!selected.control || tab) {
          event.preventDefault();
          event.stopPropagation();
        }
        selected.handlers.resume();
        selected.handlers.input({ type: "key", pressed, code: event.code, key: event.key,
          location: event.location, repeat: event.repeat });
      });
    }
    doc.defaultView.addEventListener("blur", () => {
      page.resumeKey = null;
      for (const tester of page.testers.values()) tester.pause(page.captureEnabled ? undefined : "navigation");
    });
    doc.defaultView.addEventListener("focus", () => sync());
    doc.addEventListener("visibilitychange", () => {
      if (doc.hidden) {
        page.resumeKey = null;
        for (const tester of page.testers.values()) tester.pause(page.captureEnabled ? undefined : "navigation");
      }
      else sync();
    });
  }
  page.testers.set(root, handlers);
  if (page.active !== root) handlers.pause();
  page.sync();
  return () => {
    if (page.active !== root) page.testers.get(page.active)?.pause();
    page.active = root;
    page.captureEnabled = true;
    page.sync();
  };
}
