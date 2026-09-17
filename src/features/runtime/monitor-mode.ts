import type { MonitorInfo } from './types';

function greatestCommonDivisor(left: number, right: number): number {
  let a = Math.abs(left);
  let b = Math.abs(right);
  while (b !== 0) {
    [a, b] = [b, a % b];
  }
  return a;
}

export function formatMonitorMode({ width, height }: MonitorInfo): string {
  if (!width || !height) {
    return 'RESOLUTION UNAVAILABLE';
  }
  const divisor = greatestCommonDivisor(width, height);
  return `${width} × ${height} (${width / divisor}:${height / divisor})`;
}
