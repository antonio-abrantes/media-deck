import { useEffect } from 'react';
import { WindowChassis } from '@/shared/components/WindowChassis';
import { BootTerminal } from './components/BootTerminal';
import { CoverFrame } from './components/CoverFrame';
import { HardwareStatusFooter } from './components/HardwareStatusFooter';
import { ChamferOutline } from '../../shared/components/ChamferOutline/ChamferOutline';
import {
  useMonitorInfo,
  useRuntimeSettings,
  useSessionSnapshot,
} from './hooks/useSessionSnapshot';
import {
  confirmAnimationRendered,
  confirmCoverRendered,
  resolveCloseDecision,
  simulateInsert,
} from './store/session-store';
import './RuntimeWindow.css';

export function RuntimeWindow() {
  const snapshot = useSessionSnapshot();
  const settings = useRuntimeSettings();
  const monitorInfo = useMonitorInfo();
  const reduceMotion =
    settings.reduceMotion ||
    (typeof window !== 'undefined' &&
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches);

  useEffect(() => {
    if (snapshot.state !== 'presenting' || snapshot.progress !== 100) {
      return;
    }
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(confirmAnimationRendered);
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [snapshot.progress, snapshot.state]);

  const interfaceLabel = snapshot.mountPoint
    ? `DRIVE INTERFACE ${snapshot.mountPoint}`
    : 'FLOPPY DRIVE INTERFACE v1.0';

  return (
    <WindowChassis className="runtime-window">
      <div className="runtime-layout">
        <BootTerminal
          title={snapshot.displayName}
          interfaceLabel={interfaceLabel}
          steps={snapshot.steps}
          progress={snapshot.progress}
          progressCaption={snapshot.progressCaption}
          reduceMotion={reduceMotion}
        />
        <div className="runtime-layout__footer">
          <HardwareStatusFooter
            driveLabel={snapshot.driveLabel}
            mountPoint={snapshot.mountPoint}
            state={snapshot.state}
            providerBadge={snapshot.providerBadge}
            monitorInfo={monitorInfo}
          />
          <ChamferOutline
            width={550}
            height={57}
            cut={9}
            layers={[
              { inset: 0.8, color: '#697681', strokeWidth: 1.25 },
              { inset: 4.5, color: '#46515b', strokeWidth: 1.25 },
            ]}
          />
        </div>
        <CoverFrame
          title={snapshot.displayName}
          mediaKey={snapshot.mediaKey}
          providerBadge={snapshot.providerBadge}
          coverUrl={snapshot.coverUrl}
          onCoverReady={confirmCoverRendered}
        />
      </div>

      {snapshot.state === 'idle' ? (
        <div className="runtime-actions">
          <button type="button" onClick={() => void simulateInsert()}>
            Simular inserção
          </button>
        </div>
      ) : null}

      {snapshot.closeDecisionRequired ? (
        <div
          className="close-decision"
          role="dialog"
          aria-modal="true"
          aria-labelledby="close-decision-title"
        >
          <h3 id="close-decision-title">Encerramento pendente</h3>
          <p>O processo não respondeu ao fechamento normal.</p>
          <div className="close-decision__actions">
            <button
              type="button"
              onClick={() => void resolveCloseDecision('wait')}
            >
              Esperar
            </button>
            <button
              type="button"
              onClick={() => void resolveCloseDecision('detach')}
            >
              Destacar
            </button>
            <button
              type="button"
              className="danger"
              onClick={() => void resolveCloseDecision('force')}
            >
              Forçar
            </button>
          </div>
        </div>
      ) : null}

      {snapshot.state === 'failed' || snapshot.state === 'recovery_required' ? (
        <div className="runtime-recovery" role="status">
          <p>
            Sessão interrompida
            {snapshot.errorCode ? ` · ${snapshot.errorCode}` : ''}. Nenhuma ação
            destrutiva automática.
          </p>
        </div>
      ) : null}
    </WindowChassis>
  );
}
