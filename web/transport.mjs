// SPDX-License-Identifier: GPL-3.0-or-later
// Browser effects only. Rust owns request bytes and response interpretation.
export const filters = [0x4011, 0x4015].map(productId => ({
  vendorId: 0x3151, productId, usagePage: 0xffff, usage: 2,
}));

export class HidError extends Error {
  constructor(code, message, cause) {
    super(message, { cause });
    this.code = code;
  }
}

function featureReports(collection) {
  return [...collection.featureReports,
    ...collection.children.flatMap(featureReports)];
}

export function validateDevice(device) {
  if (!filters.some(filter => filter.vendorId === device.vendorId && filter.productId === device.productId)) {
    throw new HidError("unsupported", "This is not a supported Nia87 USB device.");
  }
  const collections = device.collections.filter(item => item.usagePage === 0xffff && item.usage === 2);
  if (collections.length !== 1) {
    throw new HidError("collection", "The Nia87 configuration collection is missing or ambiguous.");
  }
  const reports = featureReports(collections[0]);
  if (reports.length !== 1 || reports[0].reportId !== 0 ||
      reports[0].items.reduce((bits, item) => bits + item.reportSize * item.reportCount, 0) !== 512) {
    throw new HidError("report-shape", "Expected one unnumbered 64-byte configuration feature report.");
  }
  return device;
}

export function selectDevice(devices) {
  const matching = devices.filter(device => filters.some(filter =>
    filter.vendorId === device.vendorId && filter.productId === device.productId) &&
    device.collections.some(item => item.usagePage === 0xffff && item.usage === 2));
  if (matching.length !== 1) {
    throw new HidError("selection", matching.length === 0
      ? "No Nia87 configuration interface selected."
      : "More than one configuration interface was selected; choose one keyboard.");
  }
  return validateDevice(matching[0]);
}

// The unnumbered Nia87 report has no ID byte in WebHID's returned DataView.
export function payloadFromView(view) {
  if (!(view instanceof DataView) || view.byteLength !== 64) {
    throw new HidError("response-shape", "Expected exactly 64 bytes from WebHID.");
  }
  return new Uint8Array(view.buffer, view.byteOffset, view.byteLength).slice();
}

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

export class Reader {
  #device;
  #codec;
  #pause;
  #tail = Promise.resolve();
  #active = true;
  #cache = null;

  constructor(device, codec, wait = pause) {
    this.#device = validateDevice(device);
    this.#codec = codec;
    this.#pause = wait;
  }

  get device() { return this.#device; }

  #check() {
    if (!this.#active) throw new HidError("disconnected", "Connection ended; select the keyboard again.");
  }

  #enqueue(operation) {
    const job = this.#tail.then(operation);
    this.#tail = job.catch(() => {});
    return job;
  }

  async #read(name) {
    this.#check();
    const request = this.#codec.read_request(name);
    if (!this.#device.opened) {
      await this.#device.open();
      this.#check();
    }
    await this.#device.sendFeatureReport(0, request);
    this.#check();
    await this.#pause(30);
    this.#check();
    const view = await this.#device.receiveFeatureReport(0);
    this.#check();
    return JSON.parse(this.#codec.decode_reply(name, payloadFromView(view)));
  }

  readAll(refresh = false) {
    return this.#enqueue(async () => {
      this.#check();
      if (!refresh && this.#cache) return this.#cache;
      const result = {};
      for (const name of ["identity", "profile", "lighting"]) {
        result[name] = await this.#read(name);
      }
      this.#cache = result;
      return result;
    });
  }

  close() {
    this.#active = false; // Reject in-flight results before awaiting browser I/O.
    this.#cache = null;
    return this.#enqueue(async () => {
      if (this.#device.opened) await this.#device.close();
    });
  }
}
