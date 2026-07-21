/// Solver options exposed in the mobile settings UI (#789).
export const SOLVER_OPTIONS = ['auto', 'sa', 'priority'] as const;
export type SolverOption = (typeof SOLVER_OPTIONS)[number];

export function solverLabel(s: SolverOption): string {
  switch (s) {
    case 'sa':
      return 'SA';
    case 'priority':
      return 'Priority (ALNS)';
    case 'auto':
    default:
      return 'Auto';
  }
}

/// Parse a non-negative integer string. Empty string resolves to 0 so the
/// server treats it as the default sentinel. Returns null for invalid input.
export function parseOptionalNonNegativeInt(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === '') return 0;
  if (!/^\d+$/.test(trimmed)) return null;
  const n = parseInt(trimmed, 10);
  if (!Number.isFinite(n) || n < 0) return null;
  return n;
}
