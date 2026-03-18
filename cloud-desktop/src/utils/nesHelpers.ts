

export function nesOperator(op: string): string {
  if (op === '=') {
    return '==';
  }
  if (op === '!=') return '!=';

  return op;
}

// TODO: maybe handle booleans here too at some point
export function nesValue(val: string): string {
  const temp = Number(val);

  if (!isNaN(temp) && isFinite(temp) && val.trim() !== '')
  {
    return String(temp);
  }

  return `"${val}"`;
}
