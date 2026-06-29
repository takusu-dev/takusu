// Brand color and theme constants

import {
  createContext,
  useContext,
  type ReactNode,
} from 'react';

export const BRAND_COLOR = '#7261A3';
export const BRAND_COLOR_LIGHT = '#9B8BC4';
export const BRAND_COLOR_DARK = '#5A4A85';

// abandonability → background color mapping for task cards
export function abandonabilityColor(abandonability: number): string {
  // 0.0 = must do (red-ish), 1.0 = can abandon (blue-ish)
  if (abandonability >= 0.75) return '#E8E0F0'; // very light purple
  if (abandonability >= 0.5) return '#F0EBE8'; // neutral
  if (abandonability >= 0.25) return '#F0E8E0'; // warm
  return '#F0E0E0'; // reddish — must do
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
  return (
    <ThemeContext.Provider value={{ dark, colors }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext);
}

export function useColors(): ColorSet {
  return useContext(ThemeContext).colors;
}
