// SPDX-License-Identifier: GPL-3.0-or-later
import test from "node:test";
import assert from "node:assert/strict";
import { notificationDevice, NotificationListener } from "./notifications.mjs";

function interfaces() {
  const configuration = { vendorId: 0x3151, productId: 0x4015, collections: [{
    usagePage: 0xffff, usage: 2, children: [], featureReports: [{ reportId: 0, items: [{ reportSize: 8, reportCount: 64 }] }],
  }] };
  const input = Object.assign(new EventTarget(), { vendorId: 0x3151, productId: 0x4015, opened: false,
    collections: [{ usagePage: 0xffff, usage: 1, children: [], inputReports: [{ reportId: 5, items: [{ reportSize: 8, reportCount: 3 }] }] }],
    async open() { this.opened = true; }, async close() { this.opened = false; },
  });
  return { configuration, input };
}

test("notification selection requires the same chooser result and exact input shape", () => {
  const { configuration, input } = interfaces();
  assert.equal(notificationDevice([configuration, input], configuration), input);
  assert.throws(() => notificationDevice([input], configuration));
  assert.throws(() => notificationDevice([configuration], configuration));
  assert.throws(() => notificationDevice([configuration, input, interfaces().input], configuration));
  input.collections[0].inputReports[0].items[0].reportCount = 4;
  assert.throws(() => notificationDevice([configuration, input], configuration));
});

test("listener preserves DataView offsets and ignores stale, wrong-device and malformed reports", async () => {
  const { input } = interfaces();
  const received = [];
  const listener = new NotificationListener(input, (id, payload) => received.push([id, [...payload]]));
  const send = overrides => input.dispatchEvent(Object.assign(new Event("inputreport"), {
    device: input, reportId: 5, data: new DataView(Uint8Array.of(99, 4, 3, 0, 99).buffer, 1, 3), ...overrides,
  }));
  await listener.open();
  send();
  send({ device: interfaces().input });
  send({ reportId: 4 });
  send({ data: new DataView(new ArrayBuffer(4)) });
  await listener.close();
  send();
  assert.deepEqual(received, [[5, [4, 3, 0]]]);
  assert.equal(input.opened, false);
});

test("closing during asynchronous open waits and closes the acquired interface", async () => {
  const { input } = interfaces();
  let release;
  input.open = async () => { await new Promise(resolve => { release = resolve; }); input.opened = true; };
  const listener = new NotificationListener(input, () => assert.fail("late report"));
  const opened = listener.open();
  const closed = listener.close();
  release();
  await Promise.all([opened, closed]);
  assert.equal(input.opened, false);
});
