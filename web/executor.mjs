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
  #receiver;
  #active = true;
  #foreground = [];
  #catalogs = [];
  #running = false;
  #recording = false;
  #idle = [];
  #currentJob = null;

  constructor(device, Operation, storage, wait = pause, codec = null) {
    this.#device = validateDevice(device);
    this.#Operation = Operation;
    this.#storage = storage;
    this.#wait = wait;
    if (device.productId === 0x4011) {
      if (!codec?.receiver_request || !codec?.decode_receiver_status ||
          !codec?.validate_receiver_command_status || !codec?.validate_receiver_identity || !codec?.receiver_limits) {
        throw new HidError("receiver-codec", "The Nia87 receiver codec is unavailable.");
      }
      this.#receiver = {
        codec, limits: JSON.parse(codec.receiver_limits()),
        reports: Object.fromEntries(["status", "select_keyboard", "release_reply", "board_identity"]
          .map(name => [name, codec.receiver_request(name)])),
      };
    }
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

  verifyReceiver() {
    if (!this.#receiver) return Promise.resolve();
    return new Promise((resolve, reject) => {
      this.#foreground.push({ identity: true, resolve, reject });
      void this.#pump();
    });
  }

  run(command) {
    return this.#enqueue(new this.#Operation(JSON.stringify(command)), Boolean(command.payload.ReadMacroCatalog), false);
  }

  // Host transactions share this exact selected-device queue and effect shell.
  // The host controller bounds pending frames before enqueueing an operation.
  runHost(operation) {
    if (this.#receiver) {
      operation.free?.();
      return Promise.reject(new HidError("receiver-host", "Host lighting over the 2.4 GHz receiver is not verified."));
    }
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

  async #send(report, onAttempt) {
    this.#check();
    const payload = Uint8Array.from(report);
    onAttempt?.();
    await this.#device.sendFeatureReport(0, payload);
    this.#check();
  }

  async #receive() {
    this.#check();
    const reply = payloadFromView(await this.#device.receiveFeatureReport(0));
    this.#check();
    return reply;
  }

  async #receiverStatus(beforeCommand) {
    await this.#send(this.#receiver.reports.status);
    await this.#wait(this.#receiver.limits.status_settle_ms);
    const reply = await this.#receive();
    const decode = beforeCommand
      ? this.#receiver.codec.validate_receiver_command_status
      : this.#receiver.codec.decode_receiver_status;
    return JSON.parse(decode(reply));
  }

  async #receiverReady() {
    const { max_polls, poll_interval_ms } = this.#receiver.limits;
    for (let attempt = 0; attempt < max_polls; attempt++) {
      if (attempt) await this.#wait(poll_interval_ms);
      if ((await this.#receiverStatus(true)).command_ready) return;
    }
    throw new HidError("receiver-timeout", "The receiver did not become ready for a keyboard command. Choose Read or reconnect.");
  }

  async #receiverReplyReady() {
    const { max_polls, poll_interval_ms } = this.#receiver.limits;
    for (let attempt = 0; attempt < max_polls; attempt++) {
      await this.#wait(poll_interval_ms);
      if ((await this.#receiverStatus(false)).reply_ready) return;
    }
    throw new HidError("receiver-timeout", "The keyboard did not return a reply through the receiver. Press a key, then choose Read or reconnect.");
  }

  async #receiverExchange(report, delayMs) {
    await this.#receiverReady();
    await this.#send(this.#receiver.reports.select_keyboard);
    await this.#send(report);
    await this.#wait(delayMs);
    await this.#receiverReplyReady();
    await this.#send(this.#receiver.reports.release_reply);
    await this.#wait(this.#receiver.limits.status_settle_ms);
    return this.#receive();
  }

  async #directExchange(report, delayMs) {
    await this.#send(report);
    await this.#wait(delayMs);
    return this.#receive();
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
          response = [...(this.#receiver
            ? await this.#receiverExchange(step.report, step.delay_ms)
            : await this.#directExchange(step.report, step.delay_ms))];
          break;
        case "write":
          await this.#open();
          if (step.before_ms) await this.#wait(step.before_ms);
          this.#check();
          if (this.#receiver) {
            await this.#receiverReady();
            await this.#send(this.#receiver.reports.select_keyboard);
          }
          try {
            await this.#send(step.report, () => { setterAttempted = true; });
          } catch (error) {
            // Only a submitted setter needs uncertain-write settling before recovery.
            if (setterAttempted) await this.#wait(Math.max(1000, step.after_ms));
            throw error;
          }
          if (step.after_ms) await this.#wait(step.after_ms);
          response = null;
          break;
        default:
          throw new Error("Unknown operation step: " + step.kind);
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
        if (job.identity) {
          try {
            await this.#open();
            const reply = await this.#receiverExchange(this.#receiver.reports.board_identity, 30);
            this.#receiver.codec.validate_receiver_identity(reply);
            job.resolve();
          } catch (error) { job.reject(error); }
          finally { this.#currentJob = null; }
          continue;
        }
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
