import type { PresentationDuration, RuntimeStep } from '../types';
import { durationTargetsMs } from '../types';

export type SequenceCatalog = {
  validating: RuntimeStep[];
  resolving: RuntimeStep[];
  presenting: RuntimeStep[];
};

export function buildSequenceCatalog(
  duration: PresentationDuration,
  reduceMotion: boolean,
): SequenceCatalog {
  const flavorMs = reduceMotion
    ? 120
    : Math.round(durationTargetsMs(duration) / 4);

  return {
    validating: [
      {
        id: 'detect',
        kind: 'operation',
        messageKey: 'runtime.step.media_detected',
        message: 'MEDIA DETECTED',
        operationId: 'validate',
      },
      {
        id: 'read_ini',
        kind: 'operation',
        messageKey: 'runtime.step.reading_profile',
        message: 'READING GAME.INI',
        operationId: 'validate',
      },
    ],
    resolving: [
      {
        id: 'resolve_profile',
        kind: 'operation',
        messageKey: 'runtime.step.resolving',
        message: 'RESOLVING LOCAL PROFILE',
        operationId: 'resolve',
      },
    ],
    presenting: [
      {
        id: 'boot_handshake',
        kind: 'flavor',
        messageKey: 'runtime.step.handshake',
        message: 'CRT HANDSHAKE',
        minimumDurationMs: flavorMs,
      },
      {
        id: 'mount_check',
        kind: 'flavor',
        messageKey: 'runtime.step.mount',
        message: 'MOUNT INTEGRITY CHECK',
        minimumDurationMs: flavorMs,
      },
      {
        id: 'session_warm',
        kind: 'flavor',
        messageKey: 'runtime.step.warm',
        message: 'WARMING SESSION BUFFERS',
        minimumDurationMs: flavorMs,
      },
      {
        id: 'ready_pulse',
        kind: 'progress',
        messageKey: 'runtime.step.ready',
        message: 'INITIALIZING GAME SESSION',
        minimumDurationMs: flavorMs,
        progress: 92,
      },
    ],
  };
}

/**
 * Runs flavor/progress steps locally. Operation steps only advance when
 * `completeOperation` is called after backend confirmation.
 */
export class SequenceEngine {
  private index = 0;
  private cancelled = false;
  private waitingOperation: string | null = null;

  constructor(
    private readonly steps: RuntimeStep[],
    private readonly onStep: (step: RuntimeStep, index: number) => void,
  ) {}

  cancel() {
    this.cancelled = true;
  }

  completeOperation(operationId: string) {
    if (this.waitingOperation === operationId) {
      this.waitingOperation = null;
    }
  }

  async run(): Promise<'completed' | 'cancelled' | 'blocked'> {
    while (this.index < this.steps.length) {
      if (this.cancelled) {
        return 'cancelled';
      }
      const step = this.steps[this.index];
      this.onStep(step, this.index);

      if (step.kind === 'operation') {
        this.waitingOperation = step.operationId ?? step.id;
        while (this.waitingOperation && !this.cancelled) {
          await delay(40);
        }
        if (this.cancelled) {
          return 'cancelled';
        }
      } else {
        const wait = step.minimumDurationMs ?? 0;
        if (wait > 0) {
          await delay(wait);
        }
      }
      this.index += 1;
    }
    return this.cancelled ? 'cancelled' : 'completed';
  }
}

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}
