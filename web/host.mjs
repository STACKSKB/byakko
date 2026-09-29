// SPDX-License-Identifier: GPL-3.0-or-later
import { prepareSampler } from "./sampler.mjs";

// One active transaction, one replaceable frame and one replaceable parameter
// update. Stop wins over both queues and retains the original Rust activity.
export class HostController {
  #codec; #dispatch; #executor; #prepare; #changed; #environment;
  #sampler = null; #activity = null; #ticket = null; #token = 0;
  #frame = null; #update = null; #stopping = false; #problem = null;
  #starting = null; #draining = null; #stopTask = null;
  state = { phase: "Idle", source: "", problem: null };
  constructor({ codec, dispatch, executor, changed = () => {}, prepare = prepareSampler, environment = globalThis }) {
    this.#codec = codec; this.#dispatch = dispatch; this.#executor = executor;
    this.#prepare = prepare; this.#changed = changed; this.#environment = environment;
  }
  get busy() { return this.state.phase !== "Idle"; }
  #phase(phase) { this.state = {...this.state, phase, problem: this.#problem}; this.#changed(this.state); }
  async #intent(input) {
    const response = await this.#dispatch(input);
    if (!response.ok) throw new Error(response.error);
    return response.outcome;
  }
  start(mode, setting, options = {}) {
    if (this.busy) return Promise.reject(new Error("Host lighting is already active."));
    const token = ++this.#token;
    this.#stopping = false; this.#problem = null; this.#phase("Preparing");
    // Start the chooser before awaiting anything, preserving the user gesture.
    const preparing = this.#prepare(mode.source, options, this.#codec, this.#environment);
    const running = this.#start(preparing, token, mode, setting).finally(() => {
      if (this.#starting === running) this.#starting = null;
    });
    this.#starting = running;
    return running;
  }
  async #start(preparing, token, mode, setting) {
    try {
      const sampler = await preparing;
      if (token !== this.#token) { await sampler.stop(); return; }
      this.#sampler = sampler;
      this.state.source = sampler.label;
      await this.#intent({type:"cancelCatalog"});
      if (token !== this.#token) {
        await sampler.stop();
        if (this.#sampler === sampler) this.#sampler = null;
        return;
      }
      this.#phase("Starting");
      const {start} = await this.#intent({type:"hostStart",mode:mode.id,setting});
      this.#ticket = start.ticket;
      const result = await this.#executor.runHost(this.#codec.BrowserHostOperation.start(JSON.stringify(start)));
      await this.#accept(result.result);
      if (!this.#activity) { await this.#finish(); return; }
      if (this.#stopping) { await this.#drain(); return; }
      this.#phase("Active");
      sampler.start(frame => {
        if (token !== this.#token || this.#stopping || !this.#activity) return;
        this.#frame = frame;
        void this.#drain();
      }, problem => { if (token === this.#token) void this.stop(problem); });
    } catch (error) { if (token === this.#token) await this.#fail(error); }
  }
  async #accept(result) {
    if (result.activity) this.#activity = result.activity;
    if (result.error) {
      this.#problem = String(result.error);
      this.#stopping = true;
    }
    if (result.event) {
      await this.#intent({type:"hostEvent",event:result.event});
      if (result.event.kind?.Finished) {
        const finished = result.event.kind.Finished;
        this.#problem = finished.problem ?? finished.restored?.Err?.message ?? this.#problem;
        this.#activity = null; this.#ticket = null;
      }
    }
  }
  async update(setting) {
    if (this.state.phase !== "Active") throw new Error("Host lighting must be running to change parameters.");
    this.#update = (await this.#intent({type:"hostUpdate",setting})).update;
    return this.#drain();
  }
  stop(problem = null) {
    if (problem) this.#problem = problem;
    if (this.#stopTask) return this.#stopTask;
    const stopping = this.#stop();
    this.#stopTask = stopping;
    void stopping.then(
      () => { if (this.#stopTask === stopping) this.#stopTask = null; },
      () => { if (this.#stopTask === stopping) this.#stopTask = null; },
    );
    return stopping;
  }
  async #stop() {
    if (this.state.phase === "Preparing") {
      ++this.#token;
      const sampler = this.#sampler; this.#sampler = null;
      await sampler?.stop();
      this.#phase("Idle");
      return;
    }
    if (!this.busy) return;
    this.#stopping = true; this.#frame = null; this.#update = null;
    this.#phase("Stopping");
    await this.#sampler?.stop(); this.#sampler = null;
    await this.#intent({type:"hostStop"});
    if (this.#starting) await this.#starting;
    else await this.#drain();
  }
  #drain() {
    if (this.#draining) return this.#draining;
    this.#draining = this.#pump().finally(() => { this.#draining = null; });
    return this.#draining;
  }
  async #pump() {
    try {
      while (this.#activity) {
        let operation;
        const activity = JSON.stringify(this.#activity);
        if (this.#stopping) {
          await this.#sampler?.stop(); this.#sampler = null;
          await this.#intent({type:"hostStop"});
          operation = this.#codec.BrowserHostOperation.stop(JSON.stringify(this.#ticket), activity, this.#problem);
        } else if (this.#update) {
          const update = this.#update; this.#update = null;
          operation = this.#codec.BrowserHostOperation.update(JSON.stringify(update), activity);
        } else if (this.#frame) {
          const frame = this.#frame; this.#frame = null;
          operation = this.#codec.BrowserHostOperation.frame(JSON.stringify(frame), activity);
        } else return;
        await this.#accept((await this.#executor.runHost(operation)).result);
      }
      await this.#finish();
    } catch (error) { await this.#fail(error); }
  }
  async #finish() {
    await this.#sampler?.stop(); this.#sampler = null;
    this.#frame = null; this.#update = null;
    this.#phase("Idle");
  }
  async #fail(error) {
    this.#problem = String(error.message ?? error);
    if (this.#activity && this.#ticket) {
      try {
        await this.#sampler?.stop(); this.#sampler = null;
        await this.#intent({type:"hostStop"});
        const operation = this.#codec.BrowserHostOperation.stop(
          JSON.stringify(this.#ticket), JSON.stringify(this.#activity), this.#problem);
        await this.#accept((await this.#executor.runHost(operation)).result);
      } catch (restoreError) {
        this.#problem += `; restoration failed: ${String(restoreError.message ?? restoreError)}`;
      }
    }
    if (this.#ticket) {
      await this.#dispatch({type:"hostEvent",event:{ticket:this.#ticket,kind:{Finished:{
        restored:{Err:{message:this.#problem,recovery:"Unverified"}},problem:null,
      }}}});
    }
    this.#activity = null; this.#ticket = null;
    await this.#finish();
  }
}
