// SPDX-License-Identifier: GPL-3.0-or-later
import { recordedKey, recordedPointer, recordingPolicy } from "./input.mjs";
import { node, button, text } from "./widgets.mjs";

// Recording is an independent local activity, with document input ownership only while active.
export function createRecorder({ doc, win, runtime, now, redraw }) {
  let recordFixed = false, recordDelay = "50", startVersion = 0;
  const heldPointerButtons = new Set();
  function reset() { startVersion++; heldPointerButtons.clear(); }
  async function stop() {
    reset();
    if (runtime.view.recording) await runtime.intent({ type: "recordStop", nowMs: Math.round(now()) });
  }
  async function start(caps) {
    const version = ++startVersion;
    try {
      const policy = recordingPolicy({ fixed: recordFixed, delay: recordDelay }, caps);
      const result = await runtime.startRecording(policy, () => version === startVersion);
      if (result.ok && version === startVersion) doc.getElementById("record-capture")?.focus();
    } catch (error) { runtime.announce(text(error), true); }
  }
  function renderControls(detail, caps, editor, countValid) {
    const recordRow = node(doc, "div", undefined, "actions");
    const fixedLabel = node(doc, "label", "Fixed delay");
    const fixedInput = node(doc, "input");
    fixedInput.type = "checkbox"; fixedInput.checked = recordFixed; fixedInput.disabled = runtime.view.recording;
    fixedInput.addEventListener("change", () => { recordFixed = fixedInput.checked; redraw(); });
    fixedLabel.prepend(fixedInput);
    recordRow.append(fixedLabel);
    if (recordFixed) {
      const delayInput = node(doc, "input");
      delayInput.type = "number"; delayInput.min = String(Math.max(1, caps.delays_ms.start));
      delayInput.max = String(caps.delays_ms.end); delayInput.value = recordDelay;
      delayInput.disabled = runtime.view.recording;
      delayInput.setAttribute("aria-label", "Fixed recording delay in milliseconds");
      delayInput.addEventListener("input", () => { recordDelay = delayInput.value; });
      recordRow.append(delayInput, node(doc, "span", "ms", "muted"));
    } else recordRow.append(node(doc, "span", `Measured timing · ${Math.min(50, caps.delays_ms.end)} ms final wait`, "muted"));
    recordRow.append(button(doc, runtime.view.recording ? "Stop recording" : "Record input", async () => {
      if (runtime.view.recording) await stop();
      else await start(caps);
    }, !runtime.view.recording && (!editor.canEdit || !countValid)));
    const capture = node(doc, "div", runtime.view.recording ? "Recording keys and pointer buttons. Click here for pointer actions; Stop releases held inputs." : "Recording is local until you save.", "record-capture");
    capture.id = "record-capture"; capture.tabIndex = 0;
    detail.append(recordRow, capture);
  }
  function onRecord(event, pressed) {
    if (!runtime.view.recording) return;
    if ((event.code === "Enter" || event.code === "Space") && event.target?.closest?.("button")) return;
    const action = recordedKey(event, pressed);
    if (!action) return;
    event.preventDefault();
    void runtime.intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  }
  const down = event => { if (!event.repeat) onRecord(event, true); };
  const up = event => onRecord(event, false);
  const pointerDown = event => {
    if (!runtime.view.recording || !event.target?.closest?.(".record-capture")) return;
    const action = recordedPointer(event, true, runtime.view.macros.capabilities);
    if (!action) return;
    event.preventDefault();
    heldPointerButtons.add(event.button);
    void runtime.intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  };
  const pointerUp = event => {
    if (!runtime.view.recording || !heldPointerButtons.delete(event.button)) return;
    const action = recordedPointer(event, false, runtime.view.macros.capabilities);
    if (action) void runtime.intent({ type: "recordInput", action, nowMs: Math.round(now()) });
  };
  const pointerCancel = () => {
    if (!runtime.view.recording) { heldPointerButtons.clear(); return; }
    for (const button of heldPointerButtons) {
      const action = recordedPointer({ button }, false, runtime.view.macros.capabilities);
      if (action) void runtime.intent({ type: "recordInput", action, nowMs: Math.round(now()) });
    }
    heldPointerButtons.clear();
  };
  const contextMenu = event => {
    if (runtime.view.recording && event.target?.closest?.(".record-capture")) event.preventDefault();
  };
  const blur = () => { if (runtime.view.recording) void stop(); };
  const visibility = () => { if (doc.hidden) blur(); };

  const listeners = [[doc, "keydown", down], [doc, "keyup", up],
    [doc, "pointerdown", pointerDown], [doc, "pointerup", pointerUp],
    [doc, "pointercancel", pointerCancel], [doc, "contextmenu", contextMenu],
    [win, "blur", blur], [doc, "visibilitychange", visibility]];
  for (const [target, event, handler] of listeners) target.addEventListener(event, handler);
  return {
    stop, reset, renderControls,
    destroy() {
      startVersion++;
      heldPointerButtons.clear();
      for (const [target, event, handler] of listeners) target.removeEventListener(event, handler);
    },
  };
}
