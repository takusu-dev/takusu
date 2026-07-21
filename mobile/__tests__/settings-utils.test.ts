import { parseOptionalNonNegativeInt, solverLabel } from '@/src/utils/settings';

describe('solverLabel', () => {
  it('returns labels for each solver option', () => {
    expect(solverLabel('auto')).toBe('Auto');
    expect(solverLabel('sa')).toBe('SA');
    expect(solverLabel('priority')).toBe('Priority (ALNS)');
  });
});

describe('parseOptionalNonNegativeInt', () => {
  it('treats empty strings as 0', () => {
    expect(parseOptionalNonNegativeInt('')).toBe(0);
    expect(parseOptionalNonNegativeInt('   ')).toBe(0);
  });

  it('parses non-negative integers', () => {
    expect(parseOptionalNonNegativeInt('0')).toBe(0);
    expect(parseOptionalNonNegativeInt('42')).toBe(42);
  });

  it('allows leading zeros', () => {
    expect(parseOptionalNonNegativeInt('00')).toBe(0);
    expect(parseOptionalNonNegativeInt('007')).toBe(7);
    expect(parseOptionalNonNegativeInt('0123')).toBe(123);
  });

  it('rejects negative numbers', () => {
    expect(parseOptionalNonNegativeInt('-1')).toBeNull();
    expect(parseOptionalNonNegativeInt(' -1 ')).toBeNull();
  });

  it('rejects decimals', () => {
    expect(parseOptionalNonNegativeInt('12.3')).toBeNull();
    expect(parseOptionalNonNegativeInt('0.0')).toBeNull();
  });

  it('rejects non-numeric input', () => {
    expect(parseOptionalNonNegativeInt('abc')).toBeNull();
    expect(parseOptionalNonNegativeInt('12a')).toBeNull();
    expect(parseOptionalNonNegativeInt('0x12')).toBeNull();
  });
});
