// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { editorAttention } from "./editor-attention.mjs";
import { assignmentFailureMessage, deviceProblemMessage, workflowProblemMessage } from "./outcome-message.mjs";

test("feature attention describes persistent conflict and recovery states", () => {
  assert.match(editorAttention({status:{Conflict:{device:{}}}}, "lighting"), /Lighting changed on the keyboard.*Revert.*Read/);
  assert.match(editorAttention({status:{Unverified:{problem:{Apply:{message:"Write failed",recovery:"Failed"}}}}}, "keymap"), /Recovery failed.*draft is retained/);
  assert.match(editorAttention({status:{Unverified:{problem:"ApplyReadbackMismatch"}}}, "settings"), /readback differed/);
  assert.equal(editorAttention({status:"Ready"}, "lighting"), null);
});

test("macro assignment notice distinguishes partial success from no save", () => {
  const problem = {Device:{Apply:{message:"Key write failed",recovery:"Verified"}}};
  assert.match(assignmentFailureMessage({macroSaved:true,problem}), /Macro saved and verified.*Key write failed/);
  assert.match(assignmentFailureMessage({macroSaved:false,problem}), /Macro was not saved or assigned/);
});

test("typed operation failures have readable guidance", () => {
  assert.match(deviceProblemMessage({Apply:{message:"Transport failed",recovery:"Verified"}}), /Save failed: Transport failed/);
  assert.match(workflowProblemMessage({Device:{Read:"Keyboard disconnected"}}), /Read failed: Keyboard disconnected/);
  assert.equal(workflowProblemMessage({Validation:"Choose another key"}), "Choose another key");
});
