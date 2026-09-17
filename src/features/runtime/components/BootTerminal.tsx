import type { RuntimeStep } from '../types';
import { ChamferOutline } from '../../../shared/components/ChamferOutline/ChamferOutline';
import './BootTerminal.css';

type BootTerminalProps = {
  title: string;
  interfaceLabel: string;
  steps: RuntimeStep[];
  progress: number;
  progressCaption: string;
  reduceMotion: boolean;
};

export function BootTerminal({
  title,
  interfaceLabel,
  steps,
  progress,
  progressCaption,
  reduceMotion,
}: BootTerminalProps) {
  const visible = steps.slice(-8);

  return (
    <section className="boot-terminal" aria-labelledby="runtime-game-title">
      <div className="boot-terminal__screen">
        <div className="boot-terminal__meta">
          <p>LAUNCH SEQUENCE</p>
          <p>{interfaceLabel}</p>
        </div>
        <h2
          id="runtime-game-title"
          className="boot-terminal__title glow-pixel-title"
        >
          {title}
        </h2>
        <div
          className={`boot-terminal__log${reduceMotion ? '' : ' boot-terminal__log--crt'}`}
          aria-live="polite"
          aria-label="Sequência de boot"
        >
          {visible.length === 0 ? (
            <p className="boot-terminal__active">
              <span aria-hidden="true">&gt;</span> AWAITING MEDIA INSERTION
            </p>
          ) : (
            visible.map((step, index) => (
              <p
                key={`${step.id}-${index}`}
                className={
                  index === visible.length - 1
                    ? 'boot-terminal__active'
                    : 'boot-terminal__complete'
                }
              >
                <span aria-hidden="true">&gt;</span>{' '}
                {step.message ?? step.messageKey}
              </p>
            ))
          )}
        </div>
        <SegmentedProgressBar value={progress} label={progressCaption} />
      </div>
      <ChamferOutline
        width={550}
        height={326}
        cut={10}
        layers={[
          { inset: 0.8, color: '#697681', strokeWidth: 1.25 },
          { inset: 4.5, color: '#46515b', strokeWidth: 1.25 },
        ]}
      />
    </section>
  );
}

function SegmentedProgressBar({
  value,
  label,
}: {
  value: number;
  label: string;
}) {
  const clamped = Math.max(0, Math.min(100, Math.round(value)));
  const segments = 24;
  const lit = Math.round((clamped / 100) * segments);

  return (
    <div className="segmented-progress">
      <div className="segmented-progress__row">
        <div
          className="segmented-progress__track"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={clamped}
          aria-label={label}
        >
          {Array.from({ length: segments }, (_, index) => (
            <span
              key={index}
              className={
                index < lit
                  ? 'segmented-progress__segment is-lit'
                  : 'segmented-progress__segment'
              }
            />
          ))}
        </div>
        <p className="segmented-progress__percent glow-amber">{clamped}%</p>
      </div>
      <p className="segmented-progress__caption glow-amber">{label}</p>
    </div>
  );
}
