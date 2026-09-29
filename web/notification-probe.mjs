// SPDX-License-Identifier: GPL-3.0-or-later
import { filters, selectDevice } from "./transport.mjs";
import { notificationDevice, NotificationListener } from "./notifications.mjs";
const evidence = { interfaces: [], reports: [] };
const status = document.getElementById("status");
const output = document.getElementById("evidence");
const choose = document.getElementById("choose");
const close = document.getElementById("close");
let listener;
const render = () => { output.textContent = JSON.stringify(evidence, null, 2); };
choose.addEventListener("click", async () => {
  choose.disabled = true;
  try {
    const selection = await navigator.hid.requestDevice({ filters: filters.map(({ vendorId, productId }) => ({ vendorId, productId })) });
    evidence.interfaces = selection.map(device => ({ productName: device.productName, vendorId: device.vendorId,
      productId: device.productId, collections: device.collections }));
    render();
    const device = notificationDevice(selection, selectDevice(selection));
    listener = new NotificationListener(device, (reportId, payload) => {
      evidence.reports.push({ at: new Date().toISOString(), reportId, payload: [...payload] });
      render();
    });
    await listener.open();
    status.textContent = "Listening for onboard changes. No feature reads or writes are sent.";
    close.disabled = false;
  } catch (error) { status.textContent = error.message; choose.disabled = false; }
});
close.addEventListener("click", async () => {
  await listener?.close(); listener = null;
  status.textContent = "Disconnected"; close.disabled = true; choose.disabled = false;
});
