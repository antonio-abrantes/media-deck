import { describe, expect, it } from 'vitest';
import { SequenceEngine, buildSequenceCatalog } from './engine';

describe('SequenceEngine', () => {
  it('bloqueia operation até confirmação e completa flavor', async () => {
    const catalog = buildSequenceCatalog('short', true);
    const seen: string[] = [];
    const engine = new SequenceEngine(
      [...catalog.validating, ...catalog.presenting.slice(0, 1)],
      (step) => {
        seen.push(step.id);
        if (step.kind === 'operation') {
          queueMicrotask(() =>
            engine.completeOperation(step.operationId ?? step.id),
          );
        }
      },
    );

    await expect(engine.run()).resolves.toBe('completed');
    expect(seen[0]).toBe('detect');
    expect(seen).toContain('boot_handshake');
  });

  it('cancela flavor sem fingir sucesso', async () => {
    const catalog = buildSequenceCatalog('cinematic', false);
    const engine = new SequenceEngine(catalog.presenting, () => {
      engine.cancel();
    });
    await expect(engine.run()).resolves.toBe('cancelled');
  });
});
