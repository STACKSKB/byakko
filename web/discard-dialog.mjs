// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button } from "./widgets.mjs";

export function createDiscardDialog({doc, win}) {
  const dialog = node(doc, "dialog", undefined, "discard-dialog");
  const controls = node(doc, "div", undefined, "actions");
  dialog.append(node(doc, "h2", "Discard macro edits?"),
    node(doc, "p", "The current macro draft has unsaved changes."), controls);
  let pending = null;
  function finish(accepted) {
    const current = pending;
    pending = null;
    current?.resolve(accepted);
  }
  const cancelled = () => finish(false);
  dialog.addEventListener("cancel", cancelled);
  dialog.addEventListener("close", cancelled);
  return {
    dialog,
    confirm() {
      if (pending) return pending.promise;
      if (typeof dialog.showModal !== "function") return Promise.resolve(Boolean(win.confirm?.("Discard unsaved macro edits?")));
      let resolve;
      const promise = new Promise(complete => { resolve = complete; });
      pending = {promise, resolve};
      controls.replaceChildren(
        button(doc, "Keep editing", () => { finish(false); dialog.close(); }),
        button(doc, "Discard changes", () => { finish(true); dialog.close(); }, false, "danger"),
      );
      dialog.showModal();
      return promise;
    },
    destroy() {
      finish(false);
      dialog.removeEventListener("cancel", cancelled);
      dialog.removeEventListener("close", cancelled);
      if (dialog.open) dialog.close();
    },
  };
}
