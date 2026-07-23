export function formatJson(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value === 'string') {
    try {
      return JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      return value;
    }
  }
  if (typeof value === 'bigint') return String(value);

  const seen = new WeakSet<object>();
  function replacer(_key: string, val: unknown): unknown {
    if (typeof val === 'bigint') return String(val);
    if (typeof val === 'object' && val !== null) {
      if (seen.has(val)) return '[Circular]';
      seen.add(val);
    }
    return val;
  }

  try {
    return JSON.stringify(value, replacer, 2);
  } catch {
    return '[Unable to serialize value]';
  }
}
