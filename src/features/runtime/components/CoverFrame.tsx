import { ChamferOutline } from '../../../shared/components/ChamferOutline/ChamferOutline';
import './CoverFrame.css';

type CoverFrameProps = {
  title: string;
  mediaKey: string | null;
  providerBadge: string;
  coverUrl?: string | null;
  onCoverReady?: () => void;
};

export function CoverFrame({
  title,
  mediaKey,
  providerBadge,
  coverUrl = null,
  onCoverReady,
}: CoverFrameProps) {
  return (
    <aside className="cover-frame" aria-label="Capa do jogo">
      <p className="cover-frame__label">GAME COVER</p>
      <div className="cover-frame__well">
        <div className="cover-frame__art">
          {coverUrl ? (
            <img src={coverUrl} alt="" onLoad={onCoverReady} />
          ) : (
            <div className="cover-frame__placeholder" aria-hidden="true">
              <span className="cover-frame__placeholder-kicker">
                MEDIADECK ARCHIVE
              </span>
              <span className="cover-frame__placeholder-mark">
                {title.slice(0, 2).toUpperCase()}
              </span>
              <span className="cover-frame__placeholder-title">{title}</span>
            </div>
          )}
          <div className="cover-frame__meta">
            <div>
              <p>{mediaKey ?? 'NO MEDIA'}</p>
              <p>FLOPPY DISK GAME SERIES</p>
            </div>
            <p className="cover-frame__badge">PROVIDED BY {providerBadge}</p>
          </div>
        </div>
      </div>
      <ChamferOutline
        width={277}
        height={389}
        cut={10}
        layers={[
          { inset: 0.8, color: '#697681', strokeWidth: 1.35 },
          { inset: 8, color: '#46515b', strokeWidth: 1.5 },
        ]}
      />
    </aside>
  );
}
