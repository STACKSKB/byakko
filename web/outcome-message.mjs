// SPDX-License-Identifier: GPL-3.0-or-later
export function deviceProblemMessage(problem) {
  if (problem === "ReadRequired") return "Read this feature first.";
  if (problem === "ApplyReadbackMismatch" || problem?.InvalidApplyResult !== undefined) return "Save unverified. Read to check.";
  if (problem?.Read !== undefined) return "Read failed. Retry Read.";
  if (problem?.Apply) {
    const recovery = {
      Verified: "Recovery verified.", Failed: "Recovery failed.",
      NotAttempted: "Recovery not attempted.", Unverified: "Recovery unverified.",
    }[problem.Apply.recovery] ?? "Check keyboard state.";
    return `Save failed. ${recovery} Read to check.`;
  }
  return "Keyboard operation failed. Read to check.";
}

export function workflowProblemMessage(problem) {
  if (problem?.Validation !== undefined) return problem.Validation;
  if (problem?.Device !== undefined) return deviceProblemMessage(problem.Device);
  return "Review the feature state before trying again.";
}

export function assignmentFailureMessage({ macroSaved, problem } = {}) {
  if (macroSaved) {
    const recovery = problem?.Device?.Apply?.recovery;
    const warning = recovery === "Failed" ? " Recovery failed." : recovery === "Unverified" ? " Recovery unverified." : "";
    return `Macro saved; key assignment failed.${warning} Check Keymap.`;
  }
  return "Macro was not saved or assigned. Check Macro and Keymap.";
}

export function assignmentSuccessMessage(detail) {
  return detail?.macroSaved ? "Macro saved and assigned (read back)." : "Macro assigned (read back).";
}
