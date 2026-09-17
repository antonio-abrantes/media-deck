export type RuntimeStepKind =
  'operation' | 'flavor' | 'progress' | 'success' | 'warning' | 'error';

export type RuntimeStep = {
  id: string;
  kind: RuntimeStepKind;
  messageKey: string;
  message?: string;
  minimumDurationMs?: number;
  operationId?: string;
  progress?: number;
};

export type PresentationDuration = 'short' | 'normal' | 'cinematic';

export type SessionSnapshot = {
  sessionId: string | null;
  state: string;
  mediaKey: string | null;
  coverArtworkId?: string | null;
  coverUrl?: string | null;
  displayName: string;
  providerBadge: string;
  driveLabel: string;
  mountPoint: string | null;
  progress: number;
  progressCaption: string;
  simulated: boolean;
  closeDecisionRequired: boolean;
  errorCode: string | null;
  steps: RuntimeStep[];
};

export type RuntimeSettings = {
  presentationDuration: PresentationDuration;
  reduceMotion: boolean;
  soundEnabled: boolean;
};

export type MonitorInfo = {
  width: number | null;
  height: number | null;
  scaleFactor: number | null;
};

export const UNKNOWN_MONITOR: MonitorInfo = {
  width: null,
  height: null,
  scaleFactor: null,
};

export const IDLE_SNAPSHOT: SessionSnapshot = {
  sessionId: null,
  state: 'idle',
  mediaKey: null,
  coverArtworkId: null,
  coverUrl: null,
  displayName: 'SYSTEM READY',
  providerBadge: 'LOCAL',
  driveLabel: 'NO DRIVE',
  mountPoint: null,
  progress: 0,
  progressCaption: 'AWAITING MEDIA',
  simulated: false,
  closeDecisionRequired: false,
  errorCode: null,
  steps: [],
};

export function durationTargetsMs(mode: PresentationDuration): number {
  switch (mode) {
    case 'short':
      return 2_000;
    case 'cinematic':
      return 10_000;
    default:
      return 5_000;
  }
}
