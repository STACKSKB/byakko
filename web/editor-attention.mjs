// SPDX-License-Identifier: GPL-3.0-or-later
// The current editor's typed status survives unrelated global notices.
export function editorAttention(editor, feature) {
  const status = editor?.status;
  if (!status || typeof status !== "object") return null;
  const label = { keymap: "Keymap", macros: "Macro", lighting: "Lighting", picture: "Per-key colors", settings: "Settings" }[feature];
  if (!label) return null;
  if (status.Conflict) return `${label} changed on the keyboard. Draft kept; Revert, then Read.`;
  const problem = status.Unverified?.problem;
  if (!problem) return null;
  if (problem === "ReadRequired") return `${label}: Read before editing.`;
  if (problem.Read !== undefined) return `${label} read failed. Retry Read.`;
  if (problem.Apply) {
    const recovery = {
      Verified: "Recovery verified.",
      Failed: "Recovery failed.",
      NotAttempted: "Recovery not attempted.",
      Unverified: "Recovery unverified.",
    }[problem.Apply.recovery] ?? "Check keyboard state.";
    return `${label} save failed. ${recovery} Draft kept; Read to check.`;
  }
  if (problem.InvalidApplyResult !== undefined || problem === "ApplyReadbackMismatch") return `${label} save unverified. Draft kept; Read to check.`;
  return `${label} needs attention. Draft kept; Read to check.`;
}

export function editorDetail(editor) {
  const problem = editor?.status?.Unverified?.problem;
  return problem?.Read ?? problem?.Apply?.message ?? problem?.InvalidApplyResult ?? "";
}
