import test from "node:test";
import assert from "node:assert/strict";
import { librarySlots } from "./macro_view.mjs";
import { librarySummary } from "./macro-form.mjs";

test("library shows configured, opaque, bound and selected slots without treating unknown as empty", () => {
  const slots = ["one", "two", "three", "four", "five"].map(id => ({ id, label: id }));
  const caps = { slots, bindings: [{ slot: "four", action: { Macro: { slot: 3, mode: 0 } } }] };
  const occupancy = { one: "Configured", two: "Opaque", three: "Unknown", four: "Unknown", five: "Empty" };
  const keymap = { base: { a: { Macro: { slot: 3, mode: 0 } } } };
  const result = librarySlots(caps, occupancy, keymap, "three");
  assert.deepEqual(result.visible.map(slot => slot.id), ["one", "two", "three", "four"]);
  assert.equal(result.bound.has("four"), true);
  assert.equal(occupancy.four, "Unknown");
});

test("library summary distinguishes unread slots from confirmed empty slots", () => {
  const slots = ["one", "two", "three"].map(id => ({id}));
  assert.equal(librarySummary(slots, {one:"Unknown", two:"Unknown", three:"Unknown"}), "Library not fully read");
  assert.equal(librarySummary(slots, {one:"Configured", two:"Unknown", three:"Opaque"}), "2 slots used · 1 unread");
  assert.equal(librarySummary(slots, {one:"Configured", two:"Empty", three:"Opaque"}), "2 of 3 slots used");
});
