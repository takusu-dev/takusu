import { Text } from 'react-native';
import { render } from '@testing-library/react-native';

import {
  abandonabilityColor,
  abandonabilityColorDark,
  abandonabilityColorCatppuccin,
  abandonabilityColorAuraSoftDark,
  abandonabilityColorFor,
  COLORS,
  DARK_COLORS,
  CATPPUCCIN_COLORS,
  AURA_SOFT_DARK_COLORS,
  HABIT_PALETTE_SIZE,
  habitColorFor,
  taskCardColor,
  ThemeProvider,
  useTheme,
} from '@/src/theme';

function ThemeProbe() {
  const { dark, theme, colors } = useTheme();
  return <Text>{`${dark ? 'dark' : 'light'}:${theme}:${colors.white}`}</Text>;
}

describe('abandonabilityColor', () => {
  it('selects the red band for values below 0.25', () => {
    expect(abandonabilityColor(-0.1)).toBe('#F2C8C8');
    expect(abandonabilityColor(0.0)).toBe('#F2C8C8');
    expect(abandonabilityColor(0.24)).toBe('#F2C8C8');
  });

  it('selects the amber band for values from 0.25 to 0.49', () => {
    expect(abandonabilityColor(0.25)).toBe('#F5E5D5');
    expect(abandonabilityColor(0.49)).toBe('#F5E5D5');
  });

  it('selects the warm neutral band for values from 0.5 to 0.74', () => {
    expect(abandonabilityColor(0.5)).toBe('#F0EDE8');
    expect(abandonabilityColor(0.74)).toBe('#F0EDE8');
  });

  it('selects the calm brand band for values from 0.75 upward', () => {
    expect(abandonabilityColor(0.75)).toBe('#EDE6F5');
    expect(abandonabilityColor(1.0)).toBe('#EDE6F5');
    expect(abandonabilityColor(1.01)).toBe('#EDE6F5');
  });
});

describe('abandonabilityColorDark', () => {
  it('selects the dark red band for values below 0.25', () => {
    expect(abandonabilityColorDark(-0.1)).toBe('#4A2E2E');
    expect(abandonabilityColorDark(0.0)).toBe('#4A2E2E');
    expect(abandonabilityColorDark(0.24)).toBe('#4A2E2E');
  });

  it('selects the dark amber band for values from 0.25 to 0.49', () => {
    expect(abandonabilityColorDark(0.25)).toBe('#423A32');
    expect(abandonabilityColorDark(0.49)).toBe('#423A32');
  });

  it('selects the dark neutral band for values from 0.5 to 0.74', () => {
    expect(abandonabilityColorDark(0.5)).toBe('#3A3A3E');
    expect(abandonabilityColorDark(0.74)).toBe('#3A3A3E');
  });

  it('selects the dark brand band for values from 0.75 upward', () => {
    expect(abandonabilityColorDark(0.75)).toBe('#3D3048');
    expect(abandonabilityColorDark(1.0)).toBe('#3D3048');
    expect(abandonabilityColorDark(1.01)).toBe('#3D3048');
  });
});

describe('abandonabilityColorCatppuccin', () => {
  it('selects the dark red band for values below 0.25', () => {
    expect(abandonabilityColorCatppuccin(-0.1)).toBe('#6A495A');
    expect(abandonabilityColorCatppuccin(0.0)).toBe('#6A495A');
    expect(abandonabilityColorCatppuccin(0.24)).toBe('#6A495A');
  });

  it('selects the warm band for values from 0.25 to 0.49', () => {
    expect(abandonabilityColorCatppuccin(0.25)).toBe('#6B645D');
    expect(abandonabilityColorCatppuccin(0.49)).toBe('#6B645D');
  });

  it('selects the neutral band for values from 0.5 to 0.74', () => {
    expect(abandonabilityColorCatppuccin(0.5)).toBe('#363A4F');
    expect(abandonabilityColorCatppuccin(0.74)).toBe('#363A4F');
  });

  it('selects the calm band for values from 0.75 upward', () => {
    expect(abandonabilityColorCatppuccin(0.75)).toBe('#494D64');
    expect(abandonabilityColorCatppuccin(1.0)).toBe('#494D64');
    expect(abandonabilityColorCatppuccin(1.01)).toBe('#494D64');
  });
});

describe('abandonabilityColorAuraSoftDark', () => {
  it('selects the dark red band for values below 0.25', () => {
    expect(abandonabilityColorAuraSoftDark(-0.1)).toBe('#462e2e');
    expect(abandonabilityColorAuraSoftDark(0.0)).toBe('#462e2e');
    expect(abandonabilityColorAuraSoftDark(0.24)).toBe('#462e2e');
  });

  it('selects the warm band for values from 0.25 to 0.49', () => {
    expect(abandonabilityColorAuraSoftDark(0.25)).toBe('#463a32');
    expect(abandonabilityColorAuraSoftDark(0.49)).toBe('#463a32');
  });

  it('selects the neutral band for values from 0.5 to 0.74', () => {
    expect(abandonabilityColorAuraSoftDark(0.5)).toBe('#29263c');
    expect(abandonabilityColorAuraSoftDark(0.74)).toBe('#29263c');
  });

  it('selects the calm band for values from 0.75 upward', () => {
    expect(abandonabilityColorAuraSoftDark(0.75)).toBe('#3d375e');
    expect(abandonabilityColorAuraSoftDark(1.0)).toBe('#3d375e');
    expect(abandonabilityColorAuraSoftDark(1.01)).toBe('#3d375e');
  });
});

describe('abandonabilityColorFor', () => {
  it('returns colors for the requested theme', () => {
    expect(abandonabilityColorFor(0.1, 'light')).toBe('#F2C8C8');
    expect(abandonabilityColorFor(0.1, 'dark')).toBe('#4A2E2E');
    expect(abandonabilityColorFor(0.1, 'catppuccin')).toBe('#6A495A');
    expect(abandonabilityColorFor(0.1, 'aura-soft-dark')).toBe('#462e2e');
    expect(abandonabilityColorFor(0.9, 'light')).toBe('#EDE6F5');
    expect(abandonabilityColorFor(0.9, 'dark')).toBe('#3D3048');
    expect(abandonabilityColorFor(0.9, 'catppuccin')).toBe('#494D64');
    expect(abandonabilityColorFor(0.9, 'aura-soft-dark')).toBe('#3d375e');
  });
});

describe('habitColorFor', () => {
  it('wraps display_id around the palette', () => {
    const base = habitColorFor(0, 'light');
    expect(habitColorFor(HABIT_PALETTE_SIZE, 'light')).toBe(base);
    expect(habitColorFor(-HABIT_PALETTE_SIZE, 'light')).toBe(base);
    expect(habitColorFor(HABIT_PALETTE_SIZE - 1, 'light')).toBe(
      habitColorFor(-1, 'light'),
    );
  });

  it('differs between light and dark mode for the same id', () => {
    expect(habitColorFor(0, 'light')).not.toBe(habitColorFor(0, 'dark'));
  });

  it('differs between dark and catppuccin for the same id', () => {
    expect(habitColorFor(0, 'dark')).not.toBe(habitColorFor(0, 'catppuccin'));
  });

  it('differs between catppuccin and aura-soft-dark for the same id', () => {
    expect(habitColorFor(0, 'catppuccin')).not.toBe(
      habitColorFor(0, 'aura-soft-dark'),
    );
  });
});

describe('taskCardColor', () => {
  it('always uses abandonability color when abandonability is below 0.25', () => {
    expect(taskCardColor(0.1, 'habit-1', 0, 'light')).toBe(
      abandonabilityColorFor(0.1, 'light'),
    );
    expect(taskCardColor(0.1, 'habit-1', 0, 'dark')).toBe(
      abandonabilityColorFor(0.1, 'dark'),
    );
    expect(taskCardColor(0.1, 'habit-1', 0, 'catppuccin')).toBe(
      abandonabilityColorFor(0.1, 'catppuccin'),
    );
    expect(taskCardColor(0.1, 'habit-1', 0, 'aura-soft-dark')).toBe(
      abandonabilityColorFor(0.1, 'aura-soft-dark'),
    );
  });

  it('uses the habit color when a habit is present and abandonability is high enough', () => {
    expect(taskCardColor(0.5, 'habit-1', 0, 'light')).toBe(
      habitColorFor(0, 'light'),
    );
    expect(taskCardColor(0.5, 'habit-1', 0, 'dark')).toBe(
      habitColorFor(0, 'dark'),
    );
    expect(taskCardColor(0.5, 'habit-1', 0, 'catppuccin')).toBe(
      habitColorFor(0, 'catppuccin'),
    );
    expect(taskCardColor(0.5, 'habit-1', 0, 'aura-soft-dark')).toBe(
      habitColorFor(0, 'aura-soft-dark'),
    );
  });

  it('falls back to abandonability color when no habit is present', () => {
    expect(taskCardColor(0.5, undefined, undefined, 'light')).toBe(
      abandonabilityColorFor(0.5, 'light'),
    );
    expect(taskCardColor(0.5, undefined, undefined, 'dark')).toBe(
      abandonabilityColorFor(0.5, 'dark'),
    );
    expect(taskCardColor(0.5, undefined, undefined, 'catppuccin')).toBe(
      abandonabilityColorFor(0.5, 'catppuccin'),
    );
    expect(taskCardColor(0.5, undefined, undefined, 'aura-soft-dark')).toBe(
      abandonabilityColorFor(0.5, 'aura-soft-dark'),
    );
  });
});

describe('ThemeProvider', () => {
  it('provides a light theme by default', async () => {
    const { getByText } = await render(
      <ThemeProvider dark={false}>
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(getByText(`light:light:${COLORS.white}`)).toBeTruthy();
  });

  it('provides a dark theme when dark is true', async () => {
    const { getByText } = await render(
      <ThemeProvider dark={true}>
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(getByText(`dark:dark:${DARK_COLORS.white}`)).toBeTruthy();
  });

  it('provides a catppuccin theme when theme is catppuccin', async () => {
    const { getByText } = await render(
      <ThemeProvider theme="catppuccin">
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(
      getByText(`dark:catppuccin:${CATPPUCCIN_COLORS.white}`),
    ).toBeTruthy();
  });

  it('provides an aura-soft-dark theme when theme is aura-soft-dark', async () => {
    const { getByText } = await render(
      <ThemeProvider theme="aura-soft-dark">
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(
      getByText(`dark:aura-soft-dark:${AURA_SOFT_DARK_COLORS.white}`),
    ).toBeTruthy();
  });
});
