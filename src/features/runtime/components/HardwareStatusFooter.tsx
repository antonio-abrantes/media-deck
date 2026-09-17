import type { MonitorInfo } from '../types';
import { formatMonitorMode } from '../monitor-mode';
import './HardwareStatusFooter.css';

type HardwareStatusFooterProps = {
  driveLabel: string;
  mountPoint: string | null;
  state: string;
  providerBadge: string;
  monitorInfo: MonitorInfo;
};

function StatusLed({ active }: { active: boolean }) {
  return (
    <span
      className={`hardware-footer__led${active ? ' is-active' : ''}`}
      aria-hidden="true"
    />
  );
}

export function HardwareStatusFooter({
  driveLabel,
  mountPoint,
  state,
  providerBadge,
  monitorInfo,
}: HardwareStatusFooterProps) {
  const driveReady = Boolean(mountPoint) || state !== 'idle';
  const providerReady =
    providerBadge.toUpperCase() !== 'LOCAL' || state !== 'idle';
  const driveName = mountPoint
    ? `DRIVE ${mountPoint.replace(/\\/g, '')}`
    : 'DRIVE';
  const monitorReady = Boolean(monitorInfo.width && monitorInfo.height);

  return (
    <footer className="hardware-footer" aria-label="Status de hardware">
      <div className="hardware-footer__cell">
        <FloppyIcon />
        <StatusLed active={driveReady} />
        <p>
          <span>{driveName}</span>
          <small>{driveLabel}</small>
        </p>
      </div>
      <div className="hardware-footer__cell">
        <ProviderIcon />
        <StatusLed active={providerReady} />
        <p>
          <span>{providerBadge.toUpperCase()}</span>
          <small>{providerReady ? 'ACTIVE' : 'STANDBY'}</small>
        </p>
      </div>
      <div className="hardware-footer__cell">
        <MonitorIcon />
        <StatusLed active={monitorReady} />
        <p>
          <span>{monitorReady ? 'MONITOR ACTIVE' : 'MONITOR UNKNOWN'}</span>
          <small>{formatMonitorMode(monitorInfo)}</small>
        </p>
      </div>
    </footer>
  );
}

function FloppyIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <rect
        x="2"
        y="1.5"
        width="12"
        height="13"
        rx="1"
        fill="none"
        stroke="currentColor"
      />
      <rect
        x="5"
        y="2.5"
        width="6"
        height="3"
        fill="currentColor"
        opacity="0.55"
      />
      <rect
        x="4"
        y="8"
        width="8"
        height="5"
        fill="none"
        stroke="currentColor"
      />
    </svg>
  );
}

function ProviderIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" />
      <path d="M5 8h6M8 5v6" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  );
}

function MonitorIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <rect
        x="1.5"
        y="2.5"
        width="13"
        height="9"
        rx="1"
        fill="none"
        stroke="currentColor"
      />
      <path d="M6 13.5h4M8 11.5v2" stroke="currentColor" />
    </svg>
  );
}
