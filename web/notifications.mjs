// SPDX-License-Identifier: GPL-3.0-or-later
// Only interfaces returned by the same physical-device chooser are siblings.
// Never combine entries from getDevices() by product name or VID/PID.
import { validateDevice, HidError } from "./transport.mjs";

function inputReports(collection) {
  return [...(collection.inputReports ?? []), ...(collection.children ?? []).flatMap(inputReports)];
}

export function notificationDevice(selection, configuration) {
  validateDevice(configuration);
  if (!selection.includes(configuration)) throw new HidError("selection", "Configuration interface is outside this selection.");
  const candidates = selection.filter(device => device.vendorId === configuration.vendorId &&
    device.productId === configuration.productId && device.collections.some(collection =>
      collection.usagePage === 0xffff && collection.usage === 1 &&
      inputReports(collection).some(report => report.reportId === 5 &&
        report.items.reduce((bits, item) => bits + item.reportSize * item.reportCount, 0) === 24)));
  if (candidates.length !== 1) throw new HidError("notifications-unavailable", candidates.length
    ? "The selected keyboard has ambiguous notification interfaces. Use manual reads."
    : "The browser did not expose this keyboard's onboard notification interface. Use manual reads.");
  return candidates[0];
}

export class NotificationListener {
  #device;
  #listener;
  #active = false;
  #openedHere = false;
  #opening = null;
  constructor(device, receive) {
    this.#device = device;
    this.#listener = event => {
      if (!this.#active || event.device !== this.#device || event.reportId !== 5 ||
          !(event.data instanceof DataView) || event.data.byteLength !== 3) return;
      receive(event.reportId, new Uint8Array(event.data.buffer, event.data.byteOffset, event.data.byteLength).slice());
    };
  }
  get device() { return this.#device; }
  open() {
    if (this.#active) return this.#opening;
    this.#device.addEventListener("inputreport", this.#listener);
    this.#active = true;
    this.#opening = this.#open();
    return this.#opening;
  }
  async #open() {
    try {
      if (!this.#device.opened) {
        await this.#device.open();
        this.#openedHere = true;
      }
    } catch (error) {
      this.#active = false;
      this.#device.removeEventListener("inputreport", this.#listener);
      throw error;
    }
  }
  async close() {
    this.#active = false;
    this.#device.removeEventListener("inputreport", this.#listener);
    await this.#opening?.catch(() => {});
    if (this.#openedHere && this.#device.opened) await this.#device.close();
    this.#openedHere = false;
  }
}
