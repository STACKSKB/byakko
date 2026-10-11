// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { editorAttention, editorDetail } from "./editor-attention.mjs";
import { assignmentFailureMessage, assignmentSuccessMessage, deviceProblemMessage, workflowProblemMessage } from "./outcome-message.mjs";

test("feature attention describes persistent conflict and recovery states", () => {
  assert.match(editorAttention({status:{Conflict:{device:{}}}}, "lighting"), /Lighting changed on the keyboard.*Revert.*Read/);
  const failed = {status:{Unverified:{problem:{Apply:{message:"Write failed",recovery:"Failed"}}}}};
  assert.match(editorAttention(failed, "keymap"), /Recovery failed.*Draft kept/);
  assert.equal(editorDetail(failed), "Write failed");
  assert.match(editorAttention({status:{Unverified:{problem:"ApplyReadbackMismatch"}}}, "settings"), /save unverified/);
  assert.equal(editorAttention({status:"Ready"}, "lighting"), null);
});

test("macro assignment notice distinguishes partial success from no save", () => {
  const problem = {Device:{Apply:{message:"Key write failed",recovery:"Verified"}}};
  assert.match(assignmentFailureMessage({macroSaved:true,problem}), /Macro saved; key assignment failed/);
  assert.match(assignmentFailureMessage({macroSaved:false,problem}), /Macro was not saved or assigned/);
  assert.equal(assignmentSuccessMessage({macroSaved:true}), "Macro saved and assigned (read back).");
  assert.equal(assignmentSuccessMessage({macroSaved:false}), "Macro assigned (read back).");
  assert.equal(assignmentSuccessMessage(null), "Macro assigned (read back).");
  assert.equal(assignmentSuccessMessage(undefined), "Macro assigned (read back).");
});

test("typed operation failures have readable guidance", () => {
  assert.match(deviceProblemMessage({Apply:{message:"Transport failed",recovery:"Verified"}}), /Save failed.*Recovery verified/);
  assert.equal(workflowProblemMessage({Device:{Read:"Keyboard disconnected"}}), "Read failed. Retry Read.");
  assert.equal(workflowProblemMessage({Validation:"Choose another key"}), "Choose another key");
});
