// SPDX-License-Identifier: GPL-3.0-or-later
// Presentation of the core's one-field settings draft. Core validates every edit.
export function pendingField(editor) {
  const original = editor?.baseline?.content?.Editable;
  if (!original) return null;
  const submitted = editor.submitted;
  const values = submitted && Object.keys(original).some(id =>
    JSON.stringify(original[id]) !== JSON.stringify(submitted[id])) ? submitted : editor.draft;
  if (!values) return null;
  return Object.keys(original).sort().find(id =>
    JSON.stringify(original[id]) !== JSON.stringify(values[id])) ?? null;
}

export function numberKind(field) {
  return field?.kind?.Number ?? null;
}

export function needsInitialRead(editor) {
  return !editor?.baseline && (editor?.status === "Unloaded" ||
    editor?.status?.Unverified?.problem === "ReadRequired");
}
