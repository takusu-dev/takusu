// Brand color and theme constants

import { createContext, useContext, useMemo, type ReactNode } from 'react';

export const BRAND_COLOR = '#7261A3';
export const BRAND_COLOR_LIGHT = '#9B8BC4';
export const BRAND_COLOR_DARK = '#5A4A85';

// abandonability → background color mapping for task cards
// 0.0 = must do (red — important), 1.0 = can abandon (calm brand-tinted)
// Palette is tuned to the purple brand color; the lowest band is clearly red
// to signal importance (Issue #188).
export function abandonabilityColor(abandonability: number): string {
  if (abandonability >= 0.75) return '#EDE6F4'; // light brand purple — calm
  if (abandonability >= 0.5) return '#F0EDE8'; // warm neutral
  if (abandonability >= 0.25) return '#F5E5D5'; // warm amber — caution
  return '#F2C8C8'; // clear red/pink — must do
}

// Dark-theme variant: dimmer, lower-saturation tints on a dark surface
export function abandonabilityColorDark(abandonability: number): string {
  if (abandonability >= 0.75) return '#2D2638'; // muted brand purple
  if (abandonability >= 0.5) return '#2A2A2E'; // neutral dark
  if (abandonability >= 0.25) return '#322A22'; // warm dark
  return '#3A1E1E'; // dark red — must do
}

// Theme-aware helper: picks the light or dark palette based on `dark`.
export function abandonabilityColorFor(
  abandonability: number,
  dark: boolean,
): string {
  return dark
    ? abandonabilityColorDark(abandonability)
    : abandonabilityColor(abandonability);
}

export const ABANDON_STEPS = [0.0, 0.25, 0.5, 0.75, 1.0] as const;

// Light theme colors (default, backward-compatible export)
export const COLORS = {
  brand: BRAND_COLOR,
  brandLight: BRAND_COLOR_LIGHT,
  brandDark: BRAND_COLOR_DARK,
  white: '#FFFFFF',
  black: '#000000',
  gray: '#888888',
  grayLight: '#CCCCCC',
  grayDark: '#444444',
  separator: '#E0E0E0',
  done: '#AAAAAA',
  red: '#E07070',
  green: '#70B070',
  surface: '#FFFFFF',
  surfaceTint: '#F8F5FC', // brand-tinted surface (dep items, etc.)
} as const;

// Dark theme colors
export const DARK_COLORS = {
  brand: BRAND_COLOR,
  brandLight: BRAND_COLOR_LIGHT,
  brandDark: BRAND_COLOR_DARK,
  white: '#1A1A2E', // dark background
  black: '#E0E0E0', // light text
  gray: '#888888',
  grayLight: '#444444',
  grayDark: '#AAAAAA',
  separator: '#333333',
  done: '#555555',
  red: '#E07070',
  green: '#70B070',
  surface: '#2A2A45', // elevated surface (buttons, cards) — lighter than bg
  surfaceTint: '#2A2438', // brand-tinted dark surface
} as const;

export type ColorSet = {
  brand: string;
  brandLight: string;
  brandDark: string;
  white: string;
  black: string;
  gray: string;
  grayLight: string;
  grayDark: string;
  separator: string;
  done: string;
  red: string;
  green: string;
  surface: string;
  surfaceTint: string;
};

// ── Theme Context ──

interface ThemeContextValue {
  dark: boolean;
  colors: ColorSet;
}

const ThemeContext = createContext<ThemeContextValue>({
  dark: false,
  colors: COLORS,
});

export function ThemeProvider({
  dark,
  children,
}: {
  dark: boolean;
  children: ReactNode;
}) {
  const colors = dark ? DARK_COLORS : COLORS;
  const value = useMemo(() => ({ dark, colors }), [dark, colors]);
  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext);
}

export function useColors(): ColorSet {
  return useContext(ThemeContext).colors;
}
