import { Text } from 'react-native';
import { render } from '@testing-library/react-native';

import {
  abandonabilityColor,
  abandonabilityColorDark,
  abandonabilityColorFor,
  COLORS,
  DARK_COLORS,
  HABIT_PALETTE_SIZE,
  habitColorFor,
  taskCardColor,
  ThemeProvider,
  useTheme,
} from '@/src/theme';

function ThemeProbe() {
  const { dark, colors } = useTheme();
  return <Text>{`${dark ? 'dark' : 'light'}:${colors.white}`}</Text>;
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
    expect(abandonabilityColor(0.75)).toBe('#EDE6F4');
    expect(abandonabilityColor(1.0)).toBe('#EDE6F4');
    expect(abandonabilityColor(1.01)).toBe('#EDE6F4');
  });
});

describe('abandonabilityColorDark', () => {
  it('selects the dark red band for values below 0.25', () => {
    expect(abandonabilityColorDark(-0.1)).toBe('#3A1E1E');
    expect(abandonabilityColorDark(0.0)).toBe('#3A1E1E');
    expect(abandonabilityColorDark(0.24)).toBe('#3A1E1E');
  });

  it('selects the dark amber band for values from 0.25 to 0.49', () => {
    expect(abandonabilityColorDark(0.25)).toBe('#322A22');
    expect(abandonabilityColorDark(0.49)).toBe('#322A22');
  });

  it('selects the dark neutral band for values from 0.5 to 0.74', () => {
    expect(abandonabilityColorDark(0.5)).toBe('#2A2A2E');
    expect(abandonabilityColorDark(0.74)).toBe('#2A2A2E');
  });

  it('selects the dark brand band for values from 0.75 upward', () => {
    expect(abandonabilityColorDark(0.75)).toBe('#2D2638');
    expect(abandonabilityColorDark(1.0)).toBe('#2D2638');
    expect(abandonabilityColorDark(1.01)).toBe('#2D2638');
  });
});

describe('abandonabilityColorFor', () => {
  it('returns light colors for light mode and dark colors for dark mode', () => {
    expect(abandonabilityColorFor(0.1, false)).toBe('#F2C8C8');
    expect(abandonabilityColorFor(0.1, true)).toBe('#3A1E1E');
    expect(abandonabilityColorFor(0.9, false)).toBe('#EDE6F4');
    expect(abandonabilityColorFor(0.9, true)).toBe('#2D2638');
  });
});

describe('habitColorFor', () => {
  it('wraps display_id around the palette', () => {
    const base = habitColorFor(0, false);
    expect(habitColorFor(HABIT_PALETTE_SIZE, false)).toBe(base);
    expect(habitColorFor(-HABIT_PALETTE_SIZE, false)).toBe(base);
    expect(habitColorFor(HABIT_PALETTE_SIZE - 1, false)).toBe(
      habitColorFor(-1, false),
    );
  });

  it('differs between light and dark mode for the same id', () => {
    expect(habitColorFor(0, false)).not.toBe(habitColorFor(0, true));
  });
});

describe('taskCardColor', () => {
  it('always uses abandonability color when abandonability is below 0.25', () => {
    expect(taskCardColor(0.1, 'habit-1', 0, false)).toBe(
      abandonabilityColorFor(0.1, false),
    );
    expect(taskCardColor(0.1, 'habit-1', 0, true)).toBe(
      abandonabilityColorFor(0.1, true),
    );
  });

  it('uses the habit color when a habit is present and abandonability is high enough', () => {
    expect(taskCardColor(0.5, 'habit-1', 0, false)).toBe(
      habitColorFor(0, false),
    );
    expect(taskCardColor(0.5, 'habit-1', 0, true)).toBe(habitColorFor(0, true));
  });

  it('falls back to abandonability color when no habit is present', () => {
    expect(taskCardColor(0.5, undefined, undefined, false)).toBe(
      abandonabilityColorFor(0.5, false),
    );
    expect(taskCardColor(0.5, undefined, undefined, true)).toBe(
      abandonabilityColorFor(0.5, true),
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

    expect(getByText(`light:${COLORS.white}`)).toBeTruthy();
  });

  it('provides a dark theme when dark is true', async () => {
    const { getByText } = await render(
      <ThemeProvider dark={true}>
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(getByText(`dark:${DARK_COLORS.white}`)).toBeTruthy();
  });
});
