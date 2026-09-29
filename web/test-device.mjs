// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only Nia87 transport. Never imported by the production bootstrap.
export class TestDevice extends EventTarget {
  vendorId = 0x3151;
  productId = 0x4015;
  productName = "Simulated Nia87";
  opened = false;
  collections = [{ usagePage: 0xffff, usage: 2, children: [], featureReports: [
    { reportId: 0, items: [{ reportSize: 8, reportCount: 64 }] },
  ] }];
  calls = [];
  base = new Uint8Array(512);
  fn = new Uint8Array(512);
  lighting = new Uint8Array(64);
  pictures = Array.from({ length: 3 }, (_, layer) => new Uint8Array(384).fill(20 + layer));
  macros = Array.from({ length: 50 }, () => new Uint8Array(256));
  request = null;
  beforeSend = null;
  beforeReceive = null;
  constructor() {
    super();
    this.lighting.set([0x87, 1, 4, 4, 7, 12, 34, 56]);
    this.lighting[63] = 0xa5;
    this.base.set([0, 0, 4, 0], 9 * 4);
    this.base.set([0, 0, 5, 0], 40 * 4);
    this.fn.set([0, 0, 41, 0], 0); // Protected Fn+Esc capture remains lossless.
  }
  async open() { this.calls.push({ kind: "open" }); this.opened = true; }
  async close() { this.calls.push({ kind: "close" }); this.opened = false; }
  async sendFeatureReport(id, data) {
    if (id !== 0 || data.length !== 64 || !this.opened) throw new Error("Unexpected test report shape/open state");
    const report = Uint8Array.from(data);
    this.calls.push({ kind: "send", report: [...report] });
    await this.beforeSend?.(report);
    if (report[0] >= 0x80) { this.request = report; return; }
    if (report[0] === 0x13 || report[0] === 0x15) {
      (report[0] === 0x13 ? this.base : this.fn).set(report.slice(8, 12), report[2] * 4);
    } else if (report[0] === 0x07) {
      this.lighting.set(report.slice(1, 8), 1);
    } else if (report[0] === 0x16) {
      this.macros[report[1]].set(report.slice(8, 8 + report[3]), report[2] * 56);
    } else if (report[0] === 0x0c) {
      const picture = this.pictures[this.lighting[4] >> 4];
      const offset = report[4] * 56;
      picture.set(report.slice(8, 8 + Math.min(56, 384 - offset)), offset);
    } else throw new Error(`Unexpected setter ${report[0]}`);
  }
  async receiveFeatureReport(id) {
    if (id !== 0 || !this.opened || !this.request) throw new Error("Unexpected test receive");
    const request = this.request;
    this.calls.push({ kind: "receive", opcode: request[0], slot: request[1], page: request[2] });
    await this.beforeReceive?.(request);
    let response;
    switch (request[0]) {
      case 0x80: response = new Uint8Array(64); response.set([0x80, 0, 1]); break;
      case 0x85: response = new Uint8Array(64); response[0] = 0x85; break;
      case 0x87: response = this.lighting.slice(); break;
      case 0x89: response = this.base.slice(request[2] * 64, (request[2] + 1) * 64); break;
      case 0x90: response = this.fn.slice(request[2] * 64, (request[2] + 1) * 64); break;
      case 0x8b: response = this.macros[request[1]].slice(request[2] * 64, (request[2] + 1) * 64); break;
      case 0x8c: response = this.pictures[this.lighting[4] >> 4].slice(request[2] * 64, (request[2] + 1) * 64); break;
      default: throw new Error(`Unexpected read ${request[0]}`);
    }
    // Deliberately use an offset DataView like native browser buffers may.
    const framed = new Uint8Array(70);
    framed.set(response, 3);
    return new DataView(framed.buffer, 3, 64);
  }
}

export class TestStore {
  records = [];
  constructor(log = []) { this.log = log; }
  async save(record, device) {
    this.log.push({ kind: "backup", feature: record.feature });
    this.records.push(structuredClone({ record, vendorId: device.vendorId, productId: device.productId }));
  }
  async all() { return structuredClone(this.records); }
}
