/**
 * Theming: a theme is a named set of design tokens applied as CSS custom
 * properties on `:root`, overriding the stylesheet's paper defaults.
 *
 * Built-ins are `paper` (light) and `ink` (dark); the preference `"system"`
 * resolves between them by `prefers-color-scheme`. Custom themes are data,
 * not code — register one with `registerTheme()` and every surface follows,
 * since the stylesheet only ever reads tokens. (No UI for custom themes yet;
 * the machinery is deliberately theme-count agnostic.)
 */

export interface Theme {
  id: string;
  label: string;
  /** Drives `color-scheme` (native controls, scrollbars, form widgets). */
  appearance: "light" | "dark";
  /** token name (without the `--` prefix) → CSS value */
  tokens: Record<string, string>;
}

const paper: Theme = {
  id: "paper",
  label: "Paper",
  appearance: "light",
  tokens: {
    paper: "#f6f2ea",
    "paper-deep": "#efe9dd",
    "paper-raised": "#fdfbf6",
    ink: "#221c12",
    "ink-soft": "#6f6754",
    "ink-faint": "#a89f8a",
    line: "#e2dac9",
    "line-strong": "#cfc5ae",
    accent: "#b0512a",
    "accent-deep": "#8d3f1f",
    danger: "#a23725",
    annotate: "#9c7c3c",
    selection: "rgba(176, 81, 42, 0.16)",
    "code-bg": "#26211a",
    "code-ink": "#ece4d2",
    shadow: "0 10px 30px rgba(34, 28, 18, 0.14), 0 2px 8px rgba(34, 28, 18, 0.08)",
  },
};

const ink: Theme = {
  id: "ink",
  label: "Ink",
  appearance: "dark",
  tokens: {
    paper: "#18140e",
    "paper-deep": "#131009",
    "paper-raised": "#211c14",
    ink: "#e9e1d0",
    "ink-soft": "#998f78",
    "ink-faint": "#6b6350",
    line: "#322b1f",
    "line-strong": "#463d2c",
    accent: "#d9824d",
    "accent-deep": "#e89a68",
    danger: "#d96b51",
    annotate: "#cda35f",
    selection: "rgba(217, 130, 77, 0.22)",
    "code-bg": "#100d08",
    "code-ink": "#d8cfba",
    shadow: "0 10px 30px rgba(0, 0, 0, 0.5), 0 2px 8px rgba(0, 0, 0, 0.35)",
  },
};

const registry = new Map<string, Theme>([
  [paper.id, paper],
  [ink.id, ink],
]);

/** Add (or replace) a theme. Takes effect on the next `applyPreference`. */
export function registerTheme(theme: Theme): void {
  registry.set(theme.id, theme);
}

export function themeIds(): string[] {
  return [...registry.keys()];
}

/** `"system"` or a registered theme id. */
export type ThemePreference = "system" | string;

const STORAGE_KEY = "karanga.theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

let applied: Theme | null = null;

export function preference(): ThemePreference {
  return localStorage.getItem(STORAGE_KEY) ?? "system";
}

function resolve(pref: ThemePreference): Theme {
  if (pref !== "system") {
    const t = registry.get(pref);
    if (t) return t;
  }
  return darkQuery.matches ? ink : paper;
}

/** Persist the preference and apply the resolved theme's tokens to `:root`. */
export function applyPreference(pref: ThemePreference): Theme {
  localStorage.setItem(STORAGE_KEY, pref);
  const theme = resolve(pref);
  const root = document.documentElement;
  // Drop the previous theme's tokens first so themes needn't share a key set.
  if (applied) {
    for (const key of Object.keys(applied.tokens)) {
      root.style.removeProperty(`--${key}`);
    }
  }
  for (const [key, value] of Object.entries(theme.tokens)) {
    root.style.setProperty(`--${key}`, value);
  }
  root.dataset.theme = theme.id;
  root.style.colorScheme = theme.appearance;
  applied = theme;
  return theme;
}

/** The toggle order: system → each registered theme → back to system. */
export function cyclePreference(): ThemePreference {
  const order: ThemePreference[] = ["system", ...themeIds()];
  const idx = order.indexOf(preference());
  return order[(idx + 1) % order.length];
}

/** Apply the saved preference and re-resolve when the OS scheme changes. */
export function initTheme(onApplied: (theme: Theme, pref: ThemePreference) => void): void {
  darkQuery.addEventListener("change", () => {
    if (preference() === "system") {
      onApplied(applyPreference("system"), "system");
    }
  });
  onApplied(applyPreference(preference()), preference());
}
