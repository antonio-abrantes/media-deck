import { type ReactNode, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ChamferOutline } from '../ChamferOutline/ChamferOutline';
import './WindowChassis.css';

type WindowChassisProps = {
  brand?: string;
  subtitle?: string;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
  outlineFollowsViewport?: boolean;
};

const RUNTIME_OUTLINE = { width: 865, height: 458 } as const;

function currentViewportOutline() {
  if (typeof window === 'undefined') return RUNTIME_OUTLINE;
  return {
    width: Math.max(1, window.innerWidth),
    height: Math.max(1, window.innerHeight),
  };
}

async function minimizeWindow() {
  try {
    await getCurrentWindow().minimize();
  } catch {
    // Browser / Vitest: no Tauri runtime.
  }
}

async function closeWindow() {
  try {
    await getCurrentWindow().close();
  } catch {
    // Browser / Vitest: no Tauri runtime.
  }
}

export function WindowChassis({
  brand = 'MEDIADECK',
  subtitle = 'PHYSICAL GAME SYSTEM',
  children,
  footer,
  className = '',
  outlineFollowsViewport = false,
}: WindowChassisProps) {
  const [viewportOutline, setViewportOutline] = useState(
    currentViewportOutline,
  );

  useEffect(() => {
    if (!outlineFollowsViewport) return;
    const update = () => setViewportOutline(currentViewportOutline());
    update();
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, [outlineFollowsViewport]);

  const outline = outlineFollowsViewport ? viewportOutline : RUNTIME_OUTLINE;

  return (
    <div className={`window-chassis ${className}`.trim()}>
      <header className="chassis-header" data-tauri-drag-region>
        <span className="chassis-header__rail" aria-hidden="true" />
        <div className="chassis-brand">
          <h1 className="chassis-title">{brand}</h1>
          <p className="chassis-subtitle">{subtitle}</p>
        </div>
        <div className="chassis-controls" data-tauri-drag-region="false">
          <button
            type="button"
            className="chassis-control chassis-control--min"
            aria-label="Minimizar"
            onClick={() => void minimizeWindow()}
          >
            <svg viewBox="0 0 12 12" aria-hidden="true" focusable="false">
              <rect x="1" y="9" width="10" height="2" fill="currentColor" />
            </svg>
          </button>
          <span
            className="chassis-control chassis-control--fixed"
            aria-label="Tamanho fixo"
            role="img"
          >
            <svg viewBox="0 0 12 12" aria-hidden="true" focusable="false">
              <rect
                x="2"
                y="2"
                width="8"
                height="8"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              />
            </svg>
          </span>
          <button
            type="button"
            className="chassis-control chassis-control--close"
            aria-label="Fechar"
            onClick={() => void closeWindow()}
          >
            <svg viewBox="0 0 12 12" aria-hidden="true" focusable="false">
              <path
                d="M2 2 L10 10 M10 2 L2 10"
                stroke="currentColor"
                strokeWidth="1.6"
                fill="none"
              />
            </svg>
          </button>
        </div>
      </header>
      <div className="chassis-body">{children}</div>
      {footer ? <div className="chassis-footer-slot">{footer}</div> : null}
      <span
        className="chassis-edge chassis-edge--top-left"
        aria-hidden="true"
      />
      <span
        className="chassis-edge chassis-edge--bottom-left"
        aria-hidden="true"
      />
      <ChamferOutline
        className="window-chassis__outline"
        width={outline.width}
        height={outline.height}
        cut={18}
        layers={[
          {
            inset: 0.8,
            color: '#887866',
            strokeWidth: 1.35,
            opacity: 0.92,
          },
        ]}
      />
    </div>
  );
}
