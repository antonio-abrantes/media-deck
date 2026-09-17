import { useLayoutEffect, useRef, useState } from 'react';
import { ChamferOutline, type ChamferOutlineLayer } from './ChamferOutline';

type ResponsiveChamferOutlineProps = {
  cut: number;
  layers: ChamferOutlineLayer[];
  className?: string;
};

/**
 * Measures the owning panel so chamfer diagonals keep their intended pixel
 * geometry while the panel resizes.
 */
export function ResponsiveChamferOutline({
  cut,
  layers,
  className,
}: ResponsiveChamferOutlineProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [size, setSize] = useState({ width: 1, height: 1 });

  useLayoutEffect(() => {
    const panel = anchorRef.current?.parentElement;
    if (!panel) return;
    const measure = () => {
      const bounds = panel.getBoundingClientRect();
      setSize({
        width: Math.max(1, bounds.width),
        height: Math.max(1, bounds.height),
      });
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(panel);
    return () => observer.disconnect();
  }, []);

  return (
    <>
      <span ref={anchorRef} hidden data-chamfer-measure />
      <ChamferOutline
        width={size.width}
        height={size.height}
        cut={cut}
        layers={layers}
        className={className}
      />
    </>
  );
}
