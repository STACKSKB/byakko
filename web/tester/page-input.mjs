// SPDX-License-Identifier: GPL-3.0-or-later
// One active tester per page, with ordinary form controls retaining their input.
const pages = new WeakMap();
const controlSelector = 'input, textarea, select, button, a, summary, [contenteditable]:not([contenteditable="false"]), [role="textbox"]';
const editingSelector = 'input, textarea, [contenteditable]:not([contenteditable="false"]), [role="textbox"]';

export function connectPageInput(root, handlers) {
  const doc = root.ownerDocument;
  let page = pages.get(doc);
  if (!page) {
    page = { testers: new Map(), active: root };
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
      if (selected.ignored || doc.hidden || !doc.hasFocus()) selected.handlers?.pause();
      else selected.handlers?.resume();
    }
    page.sync = sync;

    doc.addEventListener("pointerdown", event => sync(event.target));
    doc.addEventListener("focusin", event => sync(event.target));
    for (const [name, pressed] of [["keydown", true], ["keyup", false]]) {
      doc.addEventListener(name, event => {
        const selected = select(event.target);
        if (!selected.handlers || doc.hidden || selected.ignored) return;
        // Leave controls and Tab navigation usable; the rest of the page is the test area.
        if (!selected.control && event.code !== "Tab" && event.key !== "Tab") {
          event.preventDefault();
          event.stopPropagation();
        }
        selected.handlers.resume();
        selected.handlers.input({ type: "key", pressed, code: event.code, key: event.key,
          location: event.location, repeat: event.repeat });
      });
    }
    doc.defaultView.addEventListener("blur", () => {
      for (const tester of page.testers.values()) tester.pause();
    });
    doc.defaultView.addEventListener("focus", () => sync());
    doc.addEventListener("visibilitychange", () => {
      if (doc.hidden) for (const tester of page.testers.values()) tester.pause();
      else sync();
    });
  }
  page.testers.set(root, handlers);
  if (page.active !== root) handlers.pause();
  page.sync();
}
