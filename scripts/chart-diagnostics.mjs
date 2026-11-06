const priority = entry => {
  switch (entry.type) {
    case 'failure': return 4;
    case 'pageerror':
    case 'http-error': return 3;
    case 'error':
    case 'warning': return 2;
    case 'diagnostic-error': return 1;
    default: return 0;
  }
};

/**
 * Retain diagnostics in arrival order, preferring higher-priority entries.
 * Entries use the smoke harness's `type` field and are stored by reference so
 * asynchronous argument enrichment remains visible. Mutate entries, not the
 * returned array, and keep each entry's type stable after recording it.
 */
export function createDiagnosticBuffer(limit = 300) {
  if (!Number.isSafeInteger(limit) || limit <= 0) {
    throw new RangeError('Diagnostic buffer limit must be a positive safe integer');
  }

  const entries = [];
  const record = entry => {
    if (entries.length >= limit) {
      let oldestLowest = 0;
      for (let index = 1; index < entries.length; index += 1) {
        if (priority(entries[index]) < priority(entries[oldestLowest])) {
          oldestLowest = index;
        }
      }
      if (priority(entry) < priority(entries[oldestLowest])) return false;
      entries.splice(oldestLowest, 1);
    }
    entries.push(entry);
    return true;
  };

  return { entries, record };
}
