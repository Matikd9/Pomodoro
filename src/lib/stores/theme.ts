// Theme store.
// Applies theme colors to CSS custom properties on :root.

import type { Theme } from '$lib/types';

const OPTIONAL_KEYS = [
  '--color-background-gradient',
  '--color-heat-1',
  '--color-heat-2',
  '--color-heat-3',
  '--color-heat-4',
];

/** Apply a theme's colors to the document root CSS custom properties.
 *  Theme keys already include the `--` prefix (e.g. "--color-background"). */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;

  // Clear optional properties the incoming theme does not specify, so values from the
  // previous theme don't linger (the heatmap falls back to a ramp of the theme's accent).
  for (const key of OPTIONAL_KEYS) {
    if (!theme.colors[key]) root.style.removeProperty(key);
  }

  for (const [key, value] of Object.entries(theme.colors)) {
    root.style.setProperty(key, value);
  }
}
