import { invoke } from '@tauri-apps/api/core';

export type AdminSettings = {
  monitor_active: boolean;
  autostart: boolean;
  launcher_monitor: string;
};

export type DisplayMonitor = {
  id: string;
  label: string;
  width: number;
  height: number;
  scale_factor: number;
};

export type MaintenanceReceipt = {
  output_path: string;
  files: number;
  bytes: number;
  restart_required: boolean;
};

export type MonitorDevice = {
  id: string;
  friendly_name: string;
  drive_type: 'removable' | 'cd_rom';
  current_mount_point: string | null;
  monitor_policy: 'disabled' | 'exact_device' | 'any_optical';
  enabled: boolean;
  has_stable_identity: boolean;
  capabilities: Record<string, unknown>;
};

export const settingsApi = {
  get: () => invoke<AdminSettings>('settings_get_admin'),
  setMonitorActive: (enabled: boolean) =>
    invoke<AdminSettings>('settings_set_monitor_active', {
      request: { enabled },
    }),
  setAutostart: (enabled: boolean) =>
    invoke<AdminSettings>('settings_set_autostart', {
      request: { enabled },
    }),
  monitors: () => invoke<DisplayMonitor[]>('settings_list_monitors'),
  setLauncherMonitor: (monitorId: string) =>
    invoke<AdminSettings>('settings_set_launcher_monitor', {
      request: { monitor_id: monitorId },
    }),
  devices: () => invoke<MonitorDevice[]>('device_list'),
  configureDevice: (
    deviceId: string,
    enabled: boolean,
    monitorPolicy: MonitorDevice['monitor_policy'],
  ) =>
    invoke<MonitorDevice>('device_configure', {
      request: {
        device_id: deviceId,
        enabled,
        monitor_policy: monitorPolicy,
      },
    }),
  exportDiagnostics: (path: string) =>
    invoke<MaintenanceReceipt>('maintenance_export_diagnostics', {
      request: { path },
    }),
  exportBackup: (path: string) =>
    invoke<MaintenanceReceipt>('maintenance_export_backup', {
      request: { path },
    }),
  stageRestore: (path: string) =>
    invoke<MaintenanceReceipt>('maintenance_stage_restore', {
      request: { path },
    }),
};
