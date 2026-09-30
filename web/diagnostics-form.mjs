// SPDX-License-Identifier: GPL-3.0-or-later
import { node, button } from "./widgets.mjs";
import { createJsonDialog } from "./json-dialog.mjs";
export function createDiagnosticsForm({doc, win, runtime}) {
  const archiveDialog = createJsonDialog({doc, win, title: "Native diagnostic archive",
    description: "Review or copy the exact captured JSON before requesting a download. This view cannot restore an archive.",
    label: "Native archive JSON", filename: "byakko-native-archive.json",
    downloaded: () => runtime.announce("Archive download requested. The captured JSON remains visible for copying.")});
  function render(panel, view) {
    panel.append(node(doc, "h2", "Diagnostics"),
      node(doc, "p", "Capture the keyboard's native configuration for inspection or export. This is read-only; archives cannot be imported or restored here.", "muted"));
    const archive = view.archive;
    if (!archive) { panel.append(node(doc, "p", "This keyboard does not advertise diagnostic archives.", "muted")); return; }
    const card = node(doc, "div", undefined, "card");
    card.append(node(doc, "h3", "Native archive"));
    card.append(node(doc, "p", `Format: ${archive.capabilities.format_id} · Limit: ${archive.capabilities.max_bytes} bytes`, "muted"));
    if (archive.status.kind === "ready") card.append(node(doc, "p", `${archive.capturedBytes} bytes captured.`, "muted"));
    else if (archive.status.kind === "failed") {
      card.append(node(doc, "p", `Latest capture failed: ${archive.status.problem}`, "error"));
      if (archive.capturedBytes) card.append(node(doc, "p", `An earlier ${archive.capturedBytes}-byte capture remains available for export.`, "muted"));
    } else card.append(node(doc, "p", "No archive captured in this session.", "muted"));
    const controls = node(doc, "div", undefined, "actions");
    controls.append(button(doc, "Capture archive", () => void runtime.intent({ type: "captureArchive" }), !archive.canCapture, "primary"));
    controls.append(button(doc, "Review / export JSON", async () => {
      const result = await runtime.intent({ type: "exportArchive" });
      if (!result.ok || result.outcome?.kind !== "archiveExported") return;
      const contents = result.outcome.document;
      archiveDialog.show(contents);
      runtime.announce("Native archive JSON prepared for review. Download must be requested separately.");
    }, !archive.canExport));
    card.append(controls);
    panel.append(card);
  }

  return { render, dialog: archiveDialog.dialog };
}
