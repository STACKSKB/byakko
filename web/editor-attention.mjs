// SPDX-License-Identifier: GPL-3.0-or-later
// A feature's typed status survives unrelated global notices.
export function editorAttention(editor, feature) {
  const status = editor?.status;
  if (!status || typeof status !== "object") return null;
  const label = { keymap: "Keymap", macros: "Macro", lighting: "Lighting", picture: "Per-key colors", settings: "Settings" }[feature];
  if (!label) return null;
  if (status.Conflict) return `${label} changed on the keyboard. Your draft is retained. Review it, then use Revert to discard your draft and Read to load the keyboard state.`;
  const problem = status.Unverified?.problem;
  if (!problem) return null;
  if (problem === "ReadRequired") return `${label} needs a read before editing. Use Read to load the keyboard state.`;
  if (problem.Read !== undefined) return `${label} could not be read: ${problem.Read}. Check the connection, then use Read again.`;
  if (problem.Apply) {
    const recovery = {
      Verified: "The prior keyboard state was verified after recovery.",
      Failed: "Recovery failed; the keyboard state needs checking.",
      NotAttempted: "Recovery was not attempted; the keyboard state needs checking.",
      Unverified: "Recovery could not be verified; the keyboard state needs checking.",
    }[problem.Apply.recovery] ?? "The keyboard state needs checking.";
    return `${label} was not saved: ${problem.Apply.message}. ${recovery} Your draft is retained; use Read before trying again.`;
  }
  if (problem.InvalidApplyResult !== undefined) return `${label} could not verify the save: ${problem.InvalidApplyResult}. Your draft is retained; use Read before trying again.`;
  if (problem === "ApplyReadbackMismatch") return `${label} readback differed after saving. Your draft is retained; use Read before trying again.`;
  return `${label} needs attention. Your draft is retained; use Read to check the keyboard state.`;
}
