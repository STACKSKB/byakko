// SPDX-License-Identifier: GPL-3.0-or-later
// Geometry in standard key-width units, shared by browser keyboard surfaces.
export function boardBounds(keys) {
  return {
    width: Math.max(1, ...keys.map(key => key.x + key.width)),
    height: Math.max(1, ...keys.map(key => key.y + key.height)),
  };
}

export function keyRectangle(key, { unit = 43, gap = 4, padding = 7 } = {}) {
  return {
    left: key.x * unit + padding,
    top: key.y * unit + padding,
    width: key.width * unit - gap,
    height: key.height * unit - gap,
  };
}
