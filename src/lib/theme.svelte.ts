const KEY = "openvkcl.theme";

export type Theme = "dark" | "light";

function read(): Theme {
  return localStorage.getItem(KEY) === "light" ? "light" : "dark";
}

function apply(mode: Theme) {
  document.documentElement.dataset.theme = mode;
}

/** Appearance: dark is the default. Stored in the app like the clock's UTC/local choice. */
export const theme = $state<{ mode: Theme }>({ mode: read() });
apply(theme.mode);

export function setTheme(mode: Theme) {
  theme.mode = mode;
  localStorage.setItem(KEY, mode);
  apply(mode);
}
