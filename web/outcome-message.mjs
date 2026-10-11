// SPDX-License-Identifier: GPL-3.0-or-later
export function deviceProblemMessage(problem) {
  if (problem === "ReadRequired") return "Read this feature before trying again.";
  if (problem === "ApplyReadbackMismatch") return "The keyboard readback differed. Use Read before trying again.";
  if (problem?.Read !== undefined) return `Read failed: ${problem.Read}. Use Read to retry.`;
  if (problem?.Apply) return `Save failed: ${problem.Apply.message}. Use Read before trying again.`;
  if (problem?.InvalidApplyResult !== undefined) return `Save could not be verified: ${problem.InvalidApplyResult}. Use Read before trying again.`;
  return "The keyboard operation failed. Use Read to check its state.";
}

export function workflowProblemMessage(problem) {
  if (problem?.Validation !== undefined) return problem.Validation;
  if (problem?.Device !== undefined) return deviceProblemMessage(problem.Device);
  return "Review the feature state before trying again.";
}

export function assignmentFailureMessage({ macroSaved, problem } = {}) {
  const detail = ` ${workflowProblemMessage(problem)}`;
  return macroSaved
    ? `Macro saved and verified, but assignment to the key failed.${detail} Review the keymap and use Keymap Read before retrying the assignment.`
    : `Macro was not saved or assigned.${detail} Review the macro and keymap status, then use Read before retrying.`;
}
