import { describe, expect, it } from 'vitest';
import { formatMonitorMode } from '../monitor-mode';

describe('formatMonitorMode', () => {
  it('mostra resolução física e proporção reduzida', () => {
    expect(
      formatMonitorMode({ width: 1920, height: 1080, scaleFactor: 1.5 }),
    ).toBe('1920 × 1080 (16:9)');
  });

  it('não confunde resolução indisponível com viewport do launcher', () => {
    expect(
      formatMonitorMode({ width: null, height: null, scaleFactor: null }),
    ).toBe('RESOLUTION UNAVAILABLE');
  });
});
