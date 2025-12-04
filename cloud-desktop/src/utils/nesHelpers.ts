// helpers to translate our rule format into NES query syntax

export function nesOperator(op: string): string {
  // NES uses == instead of single = for equality check
  if (op === '=') {
    return '==';
  }
  if (op === '!=') return '!=';

  return op;
}

// TODO: maybe handle booleans here too at some point
export function nesValue(val: string): string {
  const temp = Number(val);

  // check if its a valid number
  if (!isNaN(temp) && isFinite(temp) && val.trim() !== '')
  {
    return String(temp);
  }

  return `"${val}"`;
}
