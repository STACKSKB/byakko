// SPDX-License-Identifier: GPL-3.0-or-later
// WebHID collection selection and payload framing. Rust owns protocol bytes.
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
