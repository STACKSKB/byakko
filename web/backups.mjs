// SPDX-License-Identifier: GPL-3.0-or-later
import { text } from "./widgets.mjs";
import { createJsonDialog } from "./json-dialog.mjs";
export function createBackups({doc, win, storage, runtime}) {
  const review = createJsonDialog({doc, win, title: "Diagnostic backups",
    description: "These are the exact saved records. Copy the JSON or request a download to preserve it outside this browser.",
    label: "Backup JSON", filename: "byakko-backups.json",
    downloaded: () => runtime.announce("Backup download requested. The JSON remains visible here for copying.")});
  let disposed = false;
  return {
    dialog: review.dialog,
    async show() {
      try {
        const data = await storage.all();
        if (disposed) return;
        review.show(JSON.stringify(data, null, 2), data.length !== 0);
        runtime.announce(`Prepared ${data.length} diagnostic backup records for review.`);
      } catch (error) { if (!disposed) runtime.announce(`Backup export failed: ${text(error)}`, true); }
    },
    destroy() { disposed = true; },
  };
}
