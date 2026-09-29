// SPDX-License-Identifier: GPL-3.0-or-later
// DOM KeyboardEvent.code is physical; these are USB keyboard-page usages.
const named = {
  Escape: 0x29, Enter: 0x28, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c,
  Minus: 0x2d, Equal: 0x2e, BracketLeft: 0x2f, BracketRight: 0x30,
  Backslash: 0x31, Semicolon: 0x33, Quote: 0x34, Backquote: 0x35,
  Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39,
  PrintScreen: 0x46, ScrollLock: 0x47, Pause: 0x48,
  Insert: 0x49, Home: 0x4a, PageUp: 0x4b, Delete: 0x4c,
  End: 0x4d, PageDown: 0x4e, ArrowRight: 0x4f, ArrowLeft: 0x50,
  ArrowDown: 0x51, ArrowUp: 0x52, NumLock: 0x53,
  NumpadDivide: 0x54, NumpadMultiply: 0x55, NumpadSubtract: 0x56,
  NumpadAdd: 0x57, NumpadEnter: 0x58, NumpadDecimal: 0x63,
  ControlLeft: 0xe0, ShiftLeft: 0xe1, AltLeft: 0xe2, MetaLeft: 0xe3,
  ControlRight: 0xe4, ShiftRight: 0xe5, AltRight: 0xe6, MetaRight: 0xe7,
};
for (let digit = 1; digit <= 9; digit++) named[`Digit${digit}`] = 0x1d + digit;
named.Digit0 = 0x27;
for (let digit = 0; digit <= 9; digit++) named[`Numpad${digit}`] = digit === 0 ? 0x62 : 0x58 + digit;
for (let index = 1; index <= 12; index++) named[`F${index}`] = 0x39 + index;

export function usageForCode(code) {
  if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3) - 65 + 4;
  return named[code] ?? null;
}

export function recordedKey(event, pressed) {
  const usage = usageForCode(event.code);
  return usage === null ? null : { Key: { usage, pressed } };
}
