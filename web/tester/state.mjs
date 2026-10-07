// SPDX-License-Identifier: GPL-3.0-or-later
export function emptyTest() {
  return { held: new Set(), tested: new Set(), peak: 0, last: null };
}

// Browser events are explicit inputs. Coverage is projected against the chosen layout.
export function updateTest(state, event) {
  if (event.type === "reset") return emptyTest();
  if (event.type === "release-all") return { ...state, held: new Set() };
  if (event.type !== "key") throw new Error("Unknown tester event");
  const { code, key, location, repeat, pressed } = event;
  const held = new Set(state.held);
  const tested = new Set(state.tested);
  if (code && code !== "Unidentified") {
    tested.add(code);
    if (pressed) held.add(code);
    else held.delete(code);
    // Some OS shortcuts swallow other releases while Meta is down.
    if (!pressed && (code === "MetaLeft" || code === "MetaRight")) held.clear();
  }
  return { held, tested, peak: Math.max(state.peak, held.size),
    last: { code, key, location, repeat, pressed } };
}

export function coverage(state, keys) {
  const observable = keys.filter(key => key.code !== null);
  return { tested: observable.filter(key => state.tested.has(key.code)).length, total: observable.length };
}
