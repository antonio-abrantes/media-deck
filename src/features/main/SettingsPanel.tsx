import { useEffect, useState } from 'react';
import {
  open as chooseRestorePath,
  save as chooseExportPath,
} from '@tauri-apps/plugin-dialog';
import { ResponsiveChamferOutline } from '@/shared/components/ChamferOutline/ResponsiveChamferOutline';
import {
  settingsApi,
  type AdminSettings,
  type DisplayMonitor,
  type MonitorDevice,
} from './settings-api';

const EMPTY_SETTINGS: AdminSettings = {
  monitor_active: true,
  autostart: false,
  launcher_monitor: 'auto',
};

export function SettingsPanel() {
  const [settings, setSettings] = useState(EMPTY_SETTINGS);
  const [devices, setDevices] = useState<MonitorDevice[]>([]);
  const [monitors, setMonitors] = useState<DisplayMonitor[]>([]);
  const [busy, setBusy] = useState(false);
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [notice, setNotice] = useState('Carregando configurações locais…');

  const refresh = async () => {
    const [nextSettings, nextDevices, nextMonitors] = await loadSettings();
    setSettings(nextSettings);
    setDevices(nextDevices);
    setMonitors(nextMonitors);
    setNotice('Configurações locais atualizadas.');
  };

  useEffect(() => {
    let active = true;
    void loadSettings()
      .then(([nextSettings, nextDevices, nextMonitors]) => {
        if (!active) return;
        setSettings(nextSettings);
        setDevices(nextDevices);
        setMonitors(nextMonitors);
        setNotice('Configurações locais atualizadas.');
      })
      .catch(() => {
        if (active) {
          setNotice(
            'Abra no MediaDeck para configurar monitoramento e autostart.',
          );
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const updateMonitor = async (enabled: boolean) => {
    setBusy(true);
    try {
      setSettings(await settingsApi.setMonitorActive(enabled));
      setNotice(
        enabled
          ? 'Monitoramento de mídias ativado.'
          : 'Monitoramento pausado. Nenhuma nova mídia será processada.',
      );
    } catch {
      setNotice('Não foi possível alterar o monitoramento.');
    } finally {
      setBusy(false);
    }
  };

  const updateAutostart = async (enabled: boolean) => {
    setBusy(true);
    try {
      setSettings(await settingsApi.setAutostart(enabled));
      setNotice(
        enabled
          ? 'MediaDeck iniciará com o Windows em segundo plano.'
          : 'Inicialização com o Windows desativada.',
      );
    } catch {
      setNotice('O Windows recusou a alteração de inicialização automática.');
    } finally {
      setBusy(false);
    }
  };

  const updateLauncherMonitor = async (monitorId: string) => {
    setBusy(true);
    try {
      setSettings(await settingsApi.setLauncherMonitor(monitorId));
      setNotice('Monitor de abertura do launcher atualizado.');
    } catch {
      setNotice('O monitor selecionado não está mais disponível.');
    } finally {
      setBusy(false);
    }
  };

  const updateDevice = async (
    device: MonitorDevice,
    enabled: boolean,
    policy = device.monitor_policy,
  ) => {
    setBusy(true);
    try {
      const effectivePolicy = enabled
        ? policy === 'disabled'
          ? device.drive_type === 'cd_rom' && !device.has_stable_identity
            ? 'any_optical'
            : 'exact_device'
          : policy
        : 'disabled';
      const updated = await settingsApi.configureDevice(
        device.id,
        enabled,
        effectivePolicy,
      );
      setDevices((current) =>
        current.map((item) => (item.id === updated.id ? updated : item)),
      );
      setNotice(
        enabled
          ? `${device.friendly_name} será monitorada.`
          : `${device.friendly_name} foi removida do monitoramento.`,
      );
    } catch {
      setNotice('A unidade não pôde ser configurada com essa política.');
    } finally {
      setBusy(false);
    }
  };

  const exportDiagnostics = async () => {
    const path = await chooseExportPath({
      defaultPath: 'mediadeck-diagnostics.json',
      filters: [{ name: 'MediaDeck diagnostics', extensions: ['json'] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      const receipt = await settingsApi.exportDiagnostics(path);
      setNotice(`Diagnóstico sanitizado salvo em ${receipt.output_path}.`);
    } catch {
      setNotice('Não foi possível exportar o diagnóstico.');
    } finally {
      setBusy(false);
    }
  };

  const exportBackup = async () => {
    const path = await chooseExportPath({
      defaultPath: 'mediadeck-backup.mdbak',
      filters: [{ name: 'MediaDeck backup', extensions: ['mdbak'] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      const receipt = await settingsApi.exportBackup(path);
      setNotice(
        `Backup verificado: ${receipt.files} arquivos em ${receipt.output_path}.`,
      );
    } catch {
      setNotice('Não foi possível criar o backup.');
    } finally {
      setBusy(false);
    }
  };

  const chooseRestore = async () => {
    const path = await chooseRestorePath({
      multiple: false,
      directory: false,
      filters: [{ name: 'MediaDeck backup', extensions: ['mdbak'] }],
    });
    if (typeof path === 'string') setRestorePath(path);
  };

  const confirmRestore = async () => {
    if (!restorePath) return;
    setBusy(true);
    try {
      await settingsApi.stageRestore(restorePath);
      setNotice(
        'Backup validado e preparado. Reinicie o MediaDeck para aplicar a restauração.',
      );
    } catch {
      setNotice('Backup recusado: formato, limites ou checksums inválidos.');
    } finally {
      setBusy(false);
      setRestorePath(null);
    }
  };

  return (
    <section
      id="settings-panel"
      className="settings-panel mechanical-panel"
      role="tabpanel"
      aria-label="Settings"
    >
      <ResponsiveChamferOutline
        cut={10}
        layers={[
          { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
          { inset: 4.5, color: '#343c44', strokeWidth: 1 },
        ]}
      />
      <header>
        <p className="eyebrow">FASE 9 · BACKGROUND E MÍDIAS</p>
        <h2>SETTINGS</h2>
        <button type="button" disabled={busy} onClick={() => void refresh()}>
          ATUALIZAR UNIDADES
        </button>
      </header>

      <section className="settings-group">
        <h3>EXECUÇÃO EM SEGUNDO PLANO</h3>
        <SettingSwitch
          label="MONITORAR MÍDIAS CONFIGURADAS"
          help="Quando ativo, o MediaDeck escuta somente as unidades autorizadas abaixo."
          checked={settings.monitor_active}
          disabled={busy}
          onChange={(checked) => void updateMonitor(checked)}
        />
        <SettingSwitch
          label="INICIAR O MEDIADECK COM O WINDOWS"
          help="Opt-in. Inicia minimizado na bandeja; desmarcar remove o autostart."
          checked={settings.autostart}
          disabled={busy}
          onChange={(checked) => void updateAutostart(checked)}
        />
        <label className="settings-monitor-select">
          <span>MONITOR ONDE O LAUNCHER SERÁ ABERTO</span>
          <select
            value={settings.launcher_monitor}
            disabled={busy}
            onChange={(event) => void updateLauncherMonitor(event.target.value)}
          >
            <option value="auto">Automático · monitor atual</option>
            {monitors.map((monitor) => (
              <option key={monitor.id} value={monitor.id}>
                {monitor.label}
              </option>
            ))}
          </select>
          <small>
            Usado somente quando uma mídia autorizada e um GAME.INI válido forem
            detectados.
          </small>
        </label>
        <div className="settings-maintenance">
          <strong>MANUTENÇÃO E RECUPERAÇÃO</strong>
          <button
            type="button"
            disabled={busy}
            onClick={() => void exportDiagnostics()}
          >
            EXPORTAR DIAGNÓSTICO…
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void exportBackup()}
          >
            CRIAR BACKUP…
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void chooseRestore()}
          >
            RESTAURAR BACKUP…
          </button>
        </div>
      </section>

      <section className="settings-group settings-devices">
        <h3>UNIDADES AUTORIZADAS</h3>
        <p>
          Apenas unidades marcadas podem disparar o launcher. Discos fixos e de
          rede nunca aparecem nesta lista.
        </p>
        <div className="settings-device-list">
          {devices.map((device) => {
            const canEnable =
              device.has_stable_identity || device.drive_type === 'cd_rom';
            return (
              <article key={device.id}>
                <label className="settings-device-toggle">
                  <input
                    type="checkbox"
                    checked={device.enabled}
                    disabled={busy || !canEnable}
                    onChange={(event) =>
                      void updateDevice(device, event.target.checked)
                    }
                  />
                  <span>
                    <strong>{device.friendly_name}</strong>
                    <small>
                      {driveTypeLabel(device.drive_type)} ·{' '}
                      {device.current_mount_point ?? 'SEM LETRA ATUAL'}
                    </small>
                  </span>
                </label>
                <label>
                  <span>POLÍTICA DE ESCUTA</span>
                  <select
                    value={device.monitor_policy}
                    disabled={busy || !device.enabled}
                    onChange={(event) =>
                      void updateDevice(
                        device,
                        true,
                        event.target.value as MonitorDevice['monitor_policy'],
                      )
                    }
                  >
                    <option value="exact_device" disabled={!device.has_stable_identity}>
                      Somente esta unidade física
                    </option>
                    {device.drive_type === 'cd_rom' && (
                      <option value="any_optical">Qualquer unidade óptica</option>
                    )}
                  </select>
                </label>
                {!canEnable && (
                  <small>
                    Identidade física insuficiente; configuração bloqueada por
                    segurança.
                  </small>
                )}
              </article>
            );
          })}
          {!devices.length && (
            <p>
              Nenhuma unidade removível ou óptica elegível foi detectada.
            </p>
          )}
        </div>
      </section>
      <footer role="status">{notice}</footer>
      {restorePath && (
        <aside
          className="collection-confirm"
          role="dialog"
          aria-modal="true"
          aria-labelledby="restore-confirm-title"
        >
          <section className="mechanical-panel">
            <h2 id="restore-confirm-title">PREPARAR RESTAURAÇÃO?</h2>
            <p>
              O backup será validado agora e aplicado somente no próximo
              reinício. Uma cópia de segurança dos dados atuais será criada.
            </p>
            <div>
              <button type="button" onClick={() => setRestorePath(null)}>
                CANCELAR
              </button>
              <button
                type="button"
                className="danger-action"
                onClick={() => void confirmRestore()}
              >
                CONFIRMAR RESTAURAÇÃO
              </button>
            </div>
          </section>
        </aside>
      )}
    </section>
  );
}

function SettingSwitch({
  label,
  help,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  help: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="settings-switch">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>
        <strong>{label}</strong>
        <small>{help}</small>
      </span>
    </label>
  );
}

function driveTypeLabel(type: MonitorDevice['drive_type']) {
  return type === 'cd_rom' ? 'CD/DVD' : 'REMOVÍVEL/DISQUETE';
}

function loadSettings() {
  return Promise.all([
    settingsApi.get(),
    settingsApi.devices(),
    settingsApi.monitors(),
  ]);
}
