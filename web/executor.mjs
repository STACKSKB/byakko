// SPDX-License-Identifier: GPL-3.0-or-later
import { validateDevice, payloadFromView, HidError } from "./transport.mjs";

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

// Rust decides the complete transaction, evidence and recovery. This adapter
// acknowledges each actual browser effect, including committed backup storage.
export class DeviceExecutor {
  #device;
  #Operation;
  #storage;
  #wait;
  #active = true;
  #foreground = [];
  #catalogs = [];
  #running = false;
  #recording = false;
  #idle = [];
  #currentJob = null;

  constructor(device, Operation, storage, wait = pause) {
    this.#device = validateDevice(device);
    this.#Operation = Operation;
    this.#storage = storage;
    this.#wait = wait;
  }

  get device() { return this.#device; }

  #check() {
    if (!this.#active) throw new HidError("disconnected", "The selected keyboard disconnected.");
  }

  async #open() {
    this.#check();
    if (!this.#device.opened) await this.#device.open();
    this.#check();
  }

  run(command) {
    return this.#enqueue(new this.#Operation(JSON.stringify(command)), Boolean(command.payload.ReadMacroCatalog), false);
  }

  // Host transactions share this exact selected-device queue and effect shell.
  // The host controller bounds pending frames before enqueueing an operation.
  runHost(operation) {
    return this.#enqueue(operation, false, true);
  }

  #enqueue(operation, catalog, host) {
    return new Promise((resolve, reject) => {
      const job = { operation, step: JSON.parse(operation.step()), resolve, reject,
        catalog, host, cancelled: false };
      (job.catalog ? this.#catalogs : this.#foreground).push(job);
      void this.#pump();
    });
  }

  setRecording(recording) {
    this.#recording = recording;
    if (!recording) void this.#pump();
  }

  cancelCatalog() {
    for (const job of this.#catalogs) job.cancelled = true;
    if (this.#currentJob?.catalog) this.#currentJob.cancelled = true;
    void this.#pump();
  }

  async #effect(step) {
    let setterAttempted = false;
    try {
      this.#check();
      let response;
      switch (step.kind) {
        case "backup":
          await this.#storage.save(step.record, this.#device);
          response = null;
          break;
        case "exchange":
          await this.#open();
          await this.#device.sendFeatureReport(0, Uint8Array.from(step.report));
          await this.#wait(step.delay_ms);
          this.#check();
          response = [...payloadFromView(await this.#device.receiveFeatureReport(0))];
          this.#check();
          break;
        case "write":
          await this.#open();
          if (step.before_ms) await this.#wait(step.before_ms);
          this.#check();
          try {
            setterAttempted = true;
            await this.#device.sendFeatureReport(0, Uint8Array.from(step.report));
          } catch (error) {
            // Match native uncertain-setter settling before any recovery.
            await this.#wait(Math.max(1000, step.after_ms));
            throw error;
          }
          if (step.after_ms) await this.#wait(step.after_ms);
          response = null;
          break;
        default:
          throw new Error(`Unknown operation step: ${step.kind}`);
      }
      return { Ok: response };
    } catch (error) {
      return { Err: { message: String(error.message ?? error), setterAttempted } };
    }
  }

  async #pump() {
    if (this.#running) return;
    this.#running = true;
    try {
      while (this.#foreground.length || this.#catalogs.length) {
        if (!this.#foreground.length && this.#recording && this.#active && !this.#catalogs[0].cancelled) break;
        const job = this.#foreground.shift() ?? this.#catalogs.shift();
        this.#currentJob = job;
        let yielded = false;
        try {
          while (job.step.kind !== "complete") {
            const response = job.cancelled
              ? { Err: { message: "Macro discovery cancelled", setterAttempted: false } }
              : await this.#effect(job.step);
            job.step = JSON.parse(job.operation.advance(JSON.stringify(response)));
            if (job.catalog && job.step.kind !== "complete" && job.operation.can_yield()) {
              this.#catalogs.push(job);
              yielded = true;
              // Give browser input a chance to queue foreground work between slots.
              await new Promise(resolve => setTimeout(resolve, 0));
              break;
            }
          }
          if (!yielded) job.resolve(job.host ? job.step : job.step.completion);
        } catch (error) {
          job.reject(error);
        } finally {
          this.#currentJob = null;
          if (!yielded) job.operation.free();
        }
      }
    } finally {
      this.#running = false;
      if (!this.#foreground.length && !this.#catalogs.length) {
        for (const resolve of this.#idle.splice(0)) resolve();
      }
    }
  }

  async close() {
    this.#active = false;
    const retired = this.#running || this.#foreground.length || this.#catalogs.length
      ? new Promise(resolve => this.#idle.push(resolve)) : Promise.resolve();
    void this.#pump();
    await retired;
    if (this.#device.opened) await this.#device.close();
  }
}
