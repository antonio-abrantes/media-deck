import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type {
  MonitorInfo,
  RuntimeSettings,
  RuntimeStep,
  SessionSnapshot,
} from '../types';
import { IDLE_SNAPSHOT, UNKNOWN_MONITOR } from '../types';

export type SessionStoreListener = (snapshot: SessionSnapshot) => void;

const VISUAL_PREVIEW_SNAPSHOT: SessionSnapshot = {
  sessionId: 'visual-preview',
  state: 'launching',
  mediaKey: 'NR-001',
  displayName: 'NEON RUNNER',
  providerBadge: 'STEAM',
  driveLabel: 'MEDIA INSERTED',
  mountPoint: 'A:\\',
  progress: 78,
  progressCaption: 'INITIALIZING GAME SESSION...',
  simulated: true,
  closeDecisionRequired: false,
  errorCode: null,
  steps: [
    {
      id: 'media',
      kind: 'success',
      messageKey: 'media',
      message: 'MEDIA DETECTED: A:\\',
    },
    {
      id: 'profile',
      kind: 'success',
      messageKey: 'profile',
      message: 'GAME PROFILE ........ VERIFIED',
    },
    {
      id: 'install',
      kind: 'success',
      messageKey: 'install',
      message: 'LOCAL INSTALLATION .. FOUND',
    },
    {
      id: 'scan',
      kind: 'success',
      messageKey: 'scan',
      message: 'PROCESS SCAN ........ READY',
    },
    {
      id: 'mount',
      kind: 'operation',
      messageKey: 'mount',
      message: 'MOUNTING GAME MEDIA',
    },
  ],
};

function initialSnapshot(): SessionSnapshot {
  const visualPreview =
    import.meta.env.DEV &&
    typeof window !== 'undefined' &&
    new URLSearchParams(window.location.search).get('preview') === 'prototype';

  return visualPreview ? VISUAL_PREVIEW_SNAPSHOT : IDLE_SNAPSHOT;
}

let snapshot: SessionSnapshot = initialSnapshot();
let monitorInfo: MonitorInfo = UNKNOWN_MONITOR;
let settings: RuntimeSettings = {
  presentationDuration: 'normal',
  reduceMotion: false,
  soundEnabled: false,
};
const listeners = new Set<SessionStoreListener>();
const monitorListeners = new Set<(monitor: MonitorInfo) => void>();
let started = false;
const unlistens: UnlistenFn[] = [];

function emit() {
  for (const listener of listeners) {
    listener(snapshot);
  }
}

export function getSessionSnapshot(): SessionSnapshot {
  return snapshot;
}

export function getRuntimeSettings(): RuntimeSettings {
  return settings;
}

export function getMonitorInfo(): MonitorInfo {
  return monitorInfo;
}

export function subscribeMonitor(listener: (monitor: MonitorInfo) => void) {
  monitorListeners.add(listener);
  listener(monitorInfo);
  return () => {
    monitorListeners.delete(listener);
  };
}

export function subscribeSession(listener: SessionStoreListener): () => void {
  listeners.add(listener);
  listener(snapshot);
  return () => listeners.delete(listener);
}

export function applySnapshot(next: SessionSnapshot) {
  const previousCoverId = snapshot.coverArtworkId ?? null;
  const nextCoverId = next.coverArtworkId ?? null;
  const keepCover = previousCoverId === nextCoverId;
  if (!keepCover && snapshot.coverUrl) {
    URL.revokeObjectURL(snapshot.coverUrl);
  }
  snapshot = {
    ...next,
    coverArtworkId: nextCoverId,
    coverUrl: keepCover ? (snapshot.coverUrl ?? null) : null,
  };
  emit();
  if (!keepCover && nextCoverId) {
    void refreshRuntimeCover(nextCoverId);
  }
}

export function applyStep(step: RuntimeStep) {
  const steps = [...snapshot.steps.filter((item) => item.id !== step.id), step];
  snapshot = {
    ...snapshot,
    steps,
    progress: step.progress ?? snapshot.progress,
    progressCaption: step.message ?? snapshot.progressCaption,
  };
  emit();
}

async function safeInvoke<T>(command: string, args?: Record<string, unknown>) {
  try {
    return await invoke<T>(command, args);
  } catch {
    return null;
  }
}

type RuntimeCoverDto = {
  mime_type: string;
  bytes: number[];
};

async function refreshRuntimeCover(expectedArtworkId: string) {
  const cover = await safeInvoke<RuntimeCoverDto>('runtime_get_cover');
  if (!cover || snapshot.coverArtworkId !== expectedArtworkId) {
    return;
  }
  const url = URL.createObjectURL(
    new Blob([new Uint8Array(cover.bytes)], { type: cover.mime_type }),
  );
  if (snapshot.coverUrl) {
    URL.revokeObjectURL(snapshot.coverUrl);
  }
  snapshot = { ...snapshot, coverUrl: url };
  emit();
}

export async function refreshActiveSession() {
  const active = await safeInvoke<SessionSnapshot>('session_get_active');
  if (active) {
    applySnapshot(active);
  }
}

export async function refreshSettings() {
  const value = await safeInvoke<RuntimeSettings>('settings_get_runtime');
  if (value) {
    settings = value;
    document.documentElement.classList.toggle(
      'reduce-motion',
      value.reduceMotion ||
        (typeof window.matchMedia === 'function' &&
          window.matchMedia('(prefers-reduced-motion: reduce)').matches),
    );
  }
}

function browserMonitorFallback(): MonitorInfo {
  const preview =
    import.meta.env.DEV &&
    typeof window !== 'undefined' &&
    new URLSearchParams(window.location.search).get('preview') === 'prototype';

  if (preview) {
    return { width: 1280, height: 960, scaleFactor: 1 };
  }

  if (typeof window === 'undefined' || !window.screen) {
    return UNKNOWN_MONITOR;
  }

  const scaleFactor = window.devicePixelRatio || 1;
  const width = Math.round(window.screen.width * scaleFactor);
  const height = Math.round(window.screen.height * scaleFactor);
  return width > 0 && height > 0
    ? { width, height, scaleFactor }
    : UNKNOWN_MONITOR;
}

export async function refreshMonitorInfo() {
  monitorInfo =
    (await safeInvoke<MonitorInfo>('runtime_get_monitor_info')) ??
    browserMonitorFallback();
  for (const listener of monitorListeners) {
    listener(monitorInfo);
  }
}

export async function resolveCloseDecision(
  decision: 'wait' | 'detach' | 'force',
) {
  if (!snapshot.sessionId || !snapshot.mediaKey) {
    return;
  }
  await safeInvoke('session_resolve_close', {
    request: {
      session_id: snapshot.sessionId,
      media_key: snapshot.mediaKey,
      decision,
    },
  });
  await refreshActiveSession();
}

export async function simulateInsert() {
  await safeInvoke('runtime_simulate_insert');
  await refreshActiveSession();
}

export function confirmCoverRendered() {
  void safeInvoke('runtime_cover_ready');
}

export function confirmAnimationRendered() {
  void safeInvoke('runtime_animation_ready');
}

export async function startRuntimeBridge() {
  if (started) {
    return;
  }
  started = true;
  await Promise.all([
    refreshSettings(),
    refreshActiveSession(),
    refreshMonitorInfo(),
  ]);

  const bind = async <T>(event: string, handler: (payload: T) => void) => {
    try {
      const unlisten = await listen<T>(event, (event) =>
        handler(event.payload),
      );
      unlistens.push(unlisten);
    } catch {
      // Non-Tauri environments (Vitest / browser preview).
    }
  };

  await bind<SessionSnapshot>('runtime://state_changed', applySnapshot);
  await bind<RuntimeStep>('runtime://step', applyStep);
  await bind<{ sessionId: string }>('runtime://close_decision_required', () => {
    snapshot = {
      ...snapshot,
      closeDecisionRequired: true,
      state: 'close_decision',
    };
    emit();
  });
  await bind('device://media_inserted', () => {
    void refreshActiveSession();
  });
  await bind('device://media_removed', () => {
    void refreshActiveSession();
  });
}

export function stopRuntimeBridge() {
  for (const unlisten of unlistens.splice(0)) {
    unlisten();
  }
  started = false;
}

/** Test helper: reset module state between Vitest cases. */
export function __resetSessionStoreForTests(
  next: SessionSnapshot = IDLE_SNAPSHOT,
) {
  if (snapshot.coverUrl) {
    URL.revokeObjectURL(snapshot.coverUrl);
  }
  snapshot = next;
  started = false;
  unlistens.length = 0;
  monitorInfo = UNKNOWN_MONITOR;
  emit();
}
