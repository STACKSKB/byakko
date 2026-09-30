// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button, download } from "./widgets.mjs";
export function createJsonDialog({doc, win, title, description, label, filename, downloaded}) {
  const dialog = node(doc, "dialog", undefined, "backup-dialog");
  const contents = node(doc, "textarea");
  contents.readOnly = true; contents.setAttribute("aria-label", label);
  const controls = node(doc, "div", undefined, "actions");
  dialog.append(node(doc, "h2", title), node(doc, "p", description, "muted"), contents, controls);
  return {
    dialog,
    show(document, canDownload = true) {
      contents.value = document;
      controls.replaceChildren(
        button(doc, "Download JSON", () => { download(doc, win, filename, document); downloaded(); }, !canDownload, "primary"),
        button(doc, "Close", () => {
          if (typeof dialog.close === "function") dialog.close(); else dialog.removeAttribute("open");
        }),
      );
      if (typeof dialog.showModal === "function") dialog.showModal(); else dialog.setAttribute("open", "");
    },
  };
}
