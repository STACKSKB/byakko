// SPDX-License-Identifier: GPL-3.0-or-later
// Browser appearance only; never enters the device session or backup store.
const storageKey = "byakko-appearance";
export const modes = [["system", "System"], ["light", "Light"], ["dark", "Dark"]];
export const palettes = [["indigo", "Indigo"], ["slate", "Slate"], ["forest", "Forest"], ["stackskb", "StacksKB"], ["custom", "Custom"], ["embed", "Neutral"]];
export const colorRoles = [["canvas", "Canvas"], ["paper", "Surface"], ["ink", "Text"],
  ["muted", "Muted text"], ["accent", "Accent"], ["focus", "Focus"], ["rule", "Rule"]];
const defaultColors = {
  light: {canvas:"#f4f3ef",paper:"#ffffff",ink:"#272823",muted:"#686962",accent:"#565b8f",focus:"#565b8f",rule:"#d8d8d0"},
  dark: {canvas:"#20211f",paper:"#292b28",ink:"#e5e6df",muted:"#adb0a6",accent:"#b5b9ec",focus:"#b5b9ec",rule:"#40443b"},
};

function luminance(hex) {
  const channels = [1, 3, 5].map(start => parseInt(hex.slice(start, start + 2), 16) / 255);
  return channels.map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
    .reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
}
function contrast(first, second) {
  const high = Math.max(luminance(first), luminance(second));
  const low = Math.min(luminance(first), luminance(second));
  return (high + 0.05) / (low + 0.05);
}
function mix(first, second, firstShare) {
  const channel = start => Math.round(
    parseInt(first.slice(start, start + 2), 16) * firstShare
    + parseInt(second.slice(start, start + 2), 16) * (1 - firstShare));
  return "#" + [1, 3, 5].map(start => channel(start).toString(16).padStart(2, "0")).join("");
}
function stateColors(colors, statuses) {
  const readable = (foregrounds, candidate, fallback) =>
    foregrounds.every(foreground => contrast(foreground, candidate) >= 4.5) ? candidate : fallback;
  const outlined = contrast(colors.rule, colors.paper) >= 3 ? colors.rule : colors.focus;
  return {
    hover: readable([colors.ink, statuses.error], mix(colors.paper, colors.ink, 0.94), colors.paper),
    selected: readable([colors.accent], mix(colors.paper, colors.accent, 0.86), colors.paper),
    "nav-selected": readable([colors.ink], mix(colors.canvas, colors.accent, 0.88), colors.canvas),
    "nav-hover": readable([colors.ink], mix(colors.canvas, colors.ink, 0.94), colors.canvas),
    "primary-hover": readable([colors.paper], mix(colors.ink, colors.accent, 0.8), colors.ink),
    "on-primary": colors.paper,
    "disabled-ink": colors.muted,
    "disabled-surface": colors.paper,
    "disabled-primary": colors.paper,
    "control-border": outlined,
    "dialog-border": outlined,
    "capture-border": outlined,
    "capture-surface": colors.paper,
    "border-hover": colors.focus,
  };
}
export function statusColors(colors) {
  const readable = (preferred) => ["canvas", "paper"].every(surface => contrast(preferred, colors[surface]) >= 4.5)
    ? preferred : colors.ink;
  const dark = luminance(colors.canvas) < 0.5;
  return {
    error: readable(dark ? "#f1a79d" : "#a4382c"),
    warning: readable(dark ? "#e1c486" : "#8a651f"),
    success: readable(dark ? "#a5bd99" : "#5c7455"),
    pending: readable(dark ? "#e5c78e" : "#956217"),
  };
}
export function validCustomColors(colors) {
  if (!colors || colorRoles.some(([role]) => !/^#[0-9a-f]{6}$/i.test(colors[role] ?? ""))) return false;
  return ["canvas", "paper"].every(surface =>
    contrast(colors.ink, colors[surface]) >= 4.5
    && contrast(colors.muted, colors[surface]) >= 4.5
    && contrast(colors.accent, colors[surface]) >= 4.5
    && contrast(colors.focus, colors[surface]) >= 3);
}

export function preference(value, defaults = {mode:"system",palette:"indigo"}) {
  const custom = Object.fromEntries(["light", "dark"].map(theme => [theme,
    validCustomColors(value?.custom?.[theme])
      ? Object.fromEntries(colorRoles.map(([role]) => [role, value.custom[theme][role].toLowerCase()]))
      : {...defaultColors[theme]},
  ]));
  return {
    mode: modes.some(([id]) => id === value?.mode) ? value.mode : defaults.mode,
    palette: palettes.some(([id]) => id === value?.palette) ? value.palette : defaults.palette,
    custom,
  };
}

export function resolvedTheme(mode, prefersDark) {
  return mode === "system" ? (prefersDark ? "dark" : "light") : mode;
}

export function createAppearance(doc, environment) {
  const embedded = doc.documentElement.dataset.embed === "wordpress";
  const preferenceKey = embedded ? "byakko-wordpress-appearance" : storageKey;
  let saved;
  try { saved = JSON.parse(environment.localStorage.getItem(preferenceKey)); } catch { /* Use defaults. */ }
  let current = preference(saved, embedded ? {mode:"light",palette:"embed"} : undefined);
  const media = environment.matchMedia("(prefers-color-scheme: dark)");
  let editTheme = resolvedTheme(current.mode, media.matches);
  const dialog = doc.createElement("dialog");
  dialog.className = "appearance-dialog";
  const heading = doc.createElement("h2");
  heading.id = "appearance-title";
  heading.textContent = "Appearance";
  dialog.setAttribute("aria-labelledby", heading.id);
  const status = doc.createElement("p");
  status.className = "muted";
  status.setAttribute("role", "status");
  status.textContent = "Saved in this browser. Keyboard lighting is unchanged.";
  dialog.append(heading);

  function apply() {
    const theme = resolvedTheme(current.mode, media.matches);
    doc.documentElement.dataset.theme = theme;
    doc.documentElement.dataset.palette = current.palette;
    for (const [role] of colorRoles) {
      if (current.palette === "custom") doc.documentElement.style.setProperty(`--${role}`, current.custom[theme][role]);
      else doc.documentElement.style.removeProperty(`--${role}`);
    }
    if (current.palette === "custom") {
      const colors = current.custom[theme], accent = colors.accent;
      const statuses = statusColors(colors);
      doc.documentElement.style.setProperty("--on-accent", contrast(accent, "#ffffff") >= contrast(accent, "#000000") ? "#ffffff" : "#000000");
      for (const [role, color] of Object.entries({...stateColors(colors, statuses), ...statuses})) {
        doc.documentElement.style.setProperty(`--${role}`, color);
      }
    } else {
      doc.documentElement.style.removeProperty("--on-accent");
      for (const role of [...Object.keys(stateColors(current.custom[theme], statusColors(current.custom[theme]))), "error", "warning", "success", "pending"]) {
        doc.documentElement.style.removeProperty(`--${role}`);
      }
    }
    const meta = doc.querySelector('meta[name="theme-color"]');
    if (meta) meta.content = environment.getComputedStyle(doc.documentElement).getPropertyValue("--canvas").trim();
  }
  function persist() {
    try {
      environment.localStorage.setItem(preferenceKey, JSON.stringify(current));
      status.textContent = "Saved in this browser. Keyboard lighting is unchanged.";
      status.classList.remove("error");
    } catch {
      status.textContent = "Applied for this visit. This browser could not save the preference.";
      status.classList.add("error");
    }
  }
  function selectField(title, options, value, changed) {
    const label = doc.createElement("label");
    label.className = "field";
    const caption = doc.createElement("span");
    caption.textContent = title;
    const select = doc.createElement("select");
    for (const [id, name] of options) {
      const option = doc.createElement("option");
      option.value = id; option.textContent = name;
      select.append(option);
    }
    select.value = value;
    select.addEventListener("change", () => changed(select.value));
    label.append(caption, select);
    return label;
  }
  dialog.append(selectField("Theme", modes, current.mode, value => {
    current = {...current, mode:value}; apply(); persist();
  }));
  const choices = palettes.filter(([id]) => embedded || id !== "embed");
  const customControls = doc.createElement("div");
  dialog.append(selectField("Colour palette", choices, current.palette, value => {
    current = {...current, palette:value};
    customControls.hidden = value !== "custom";
    apply(); persist();
  }));
  customControls.hidden = current.palette !== "custom";
  const colorInputs = new Map();
  customControls.append(selectField("Edit custom colours for", [["light", "Light"], ["dark", "Dark"]], editTheme, value => {
    editTheme = value;
    for (const [role, input] of colorInputs) input.value = current.custom[editTheme][role];
  }));
  for (const [role, title] of colorRoles) {
    const label = doc.createElement("label");
    label.className = "field";
    const caption = doc.createElement("span");
    caption.textContent = title;
    const input = doc.createElement("input");
    input.type = "color";
    input.value = current.custom[editTheme][role];
    label.append(caption, input);
    customControls.append(label);
    colorInputs.set(role, input);
  }
  const saveColors = doc.createElement("button");
  saveColors.type = "button";
  saveColors.textContent = "Save custom colours";
  saveColors.addEventListener("click", () => {
    const colors = Object.fromEntries([...colorInputs].map(([role, input]) => [role, input.value]));
    if (!validCustomColors(colors)) {
      status.textContent = "Choose contrasting text, muted text, accent and focus colours before saving.";
      status.classList.add("error");
      return;
    }
    current = {...current, custom:{...current.custom, [editTheme]:colors}};
    apply(); persist();
  });
  customControls.append(saveColors);
  dialog.append(customControls);
  const actions = doc.createElement("div");
  actions.className = "actions";
  const close = doc.createElement("button");
  close.type = "button"; close.textContent = "Done";
  close.addEventListener("click", () => dialog.close());
  actions.append(close);
  dialog.append(status, actions);
  const button = doc.createElement("button");
  button.type = "button"; button.textContent = "Appearance";
  button.addEventListener("click", () => dialog.showModal());
  media.addEventListener("change", apply);
  apply();
  return {button, dialog, destroy: () => media.removeEventListener("change", apply)};
}
