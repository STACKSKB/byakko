// SPDX-License-Identifier: GPL-3.0-or-later
// Browser appearance only; never enters the device session or backup store.
const storageKey = "byakko-appearance";
export const modes = [["system", "System"], ["light", "Light"], ["dark", "Dark"]];
export const palettes = [["indigo", "Indigo"], ["slate", "Slate"], ["forest", "Forest"], ["embed", "hosting site"]];

export function preference(value, defaults = { mode: "system", palette: "indigo" }) {
  return {
    mode: modes.some(([id]) => id === value?.mode) ? value.mode : defaults.mode,
    palette: palettes.some(([id]) => id === value?.palette) ? value.palette : defaults.palette,
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
  let current = preference(saved, embedded ? { mode: "light", palette: "embed" } : undefined);
  const media = environment.matchMedia("(prefers-color-scheme: dark)");
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
    const meta = doc.querySelector('meta[name="theme-color"]');
    if (meta) meta.content = environment.getComputedStyle(doc.documentElement).getPropertyValue("--canvas").trim();
  }
  const choices = palettes.filter(([id]) => embedded || id !== "embed");
  for (const [key, title, options] of [["mode", "Theme", modes], ["palette", "Colour palette", choices]]) {
    const label = doc.createElement("label");
    label.className = "field";
    const caption = doc.createElement("span");
    caption.textContent = title;
    const select = doc.createElement("select");
    for (const [value, text] of options) {
      const option = doc.createElement("option");
      option.value = value; option.textContent = text;
      select.append(option);
    }
    select.value = current[key];
    select.addEventListener("change", () => {
      current = { ...current, [key]: select.value };
      apply();
      try {
        environment.localStorage.setItem(preferenceKey, JSON.stringify(current));
        status.textContent = "Saved in this browser. Keyboard lighting is unchanged.";
      } catch {
        status.textContent = "Applied for this visit. This browser could not save the preference.";
      }
    });
    label.append(caption, select);
    dialog.append(label);
  }
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
  return { button, dialog, destroy: () => media.removeEventListener("change", apply) };
}
