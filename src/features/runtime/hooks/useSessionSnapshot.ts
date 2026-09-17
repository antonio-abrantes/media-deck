import { useEffect, useState } from 'react';
import {
  getMonitorInfo,
  getRuntimeSettings,
  getSessionSnapshot,
  refreshMonitorInfo,
  startRuntimeBridge,
  subscribeMonitor,
  subscribeSession,
} from '../store/session-store';
import type { MonitorInfo, RuntimeSettings, SessionSnapshot } from '../types';

export function useSessionSnapshot(): SessionSnapshot {
  const [snapshot, setSnapshot] = useState(getSessionSnapshot);

  useEffect(() => {
    void startRuntimeBridge();
    return subscribeSession(setSnapshot);
  }, []);

  return snapshot;
}

export function useRuntimeSettings(): RuntimeSettings {
  const [value, setValue] = useState(getRuntimeSettings);

  useEffect(() => {
    void startRuntimeBridge().then(() => setValue(getRuntimeSettings()));
  }, []);

  return value;
}

export function useMonitorInfo(): MonitorInfo {
  const [value, setValue] = useState(getMonitorInfo);

  useEffect(() => {
    void startRuntimeBridge().then(() => {
      setValue(getMonitorInfo());
      void refreshMonitorInfo();
    });
    return subscribeMonitor(setValue);
  }, []);

  return value;
}
