import './ChamferOutline.css';

export type ChamferOutlineLayer = {
  inset: number;
  color: string;
  strokeWidth?: number;
  opacity?: number;
};

type ChamferOutlineProps = {
  width: number;
  height: number;
  cut: number;
  layers: ChamferOutlineLayer[];
  className?: string;
};

function polygonPoints(
  width: number,
  height: number,
  cut: number,
  inset: number,
) {
  return [
    [cut, inset],
    [width - cut, inset],
    [width - inset, cut],
    [width - inset, height - cut],
    [width - cut, height - inset],
    [cut, height - inset],
    [inset, height - cut],
    [inset, cut],
  ]
    .map(([x, y]) => `${x},${y}`)
    .join(' ');
}

export function ChamferOutline({
  width,
  height,
  cut,
  layers,
  className = '',
}: ChamferOutlineProps) {
  return (
    <svg
      className={`chamfer-outline ${className}`.trim()}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      aria-hidden="true"
      focusable="false"
      data-chamfer-outline
    >
      {layers.map((layer, index) => (
        <polygon
          key={`${layer.inset}-${index}`}
          points={polygonPoints(width, height, cut, layer.inset)}
          fill="none"
          stroke={layer.color}
          strokeWidth={layer.strokeWidth ?? 1}
          opacity={layer.opacity ?? 1}
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}
