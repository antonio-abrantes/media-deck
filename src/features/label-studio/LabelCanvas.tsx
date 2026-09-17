import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import Konva from 'konva';
import {
  Circle,
  Group,
  Image as KonvaImage,
  Layer,
  Line,
  Rect,
  Stage,
  Text,
  Transformer,
} from 'react-konva';
import {
  exportPixelDimensions,
  mmToPx,
  PREVIEW_DPI,
  pxToMm,
  snapPosition,
} from './engine';
import { useLabelStudioStore } from './store';
import type { ArtworkAsset, NormalizedCrop, SceneElement } from './types';

export type LabelStageHandle = {
  getStage: () => Konva.Stage | null;
  renderExport: (dpi: number) => string;
  renderThumbnail: () => string;
};

type Size = { width: number; height: number };

export const LabelCanvas = forwardRef<LabelStageHandle>(
  function LabelCanvas(_props, forwardedRef) {
    const hostRef = useRef<HTMLDivElement>(null);
    const stageRef = useRef<Konva.Stage>(null);
    const transformerRef = useRef<Konva.Transformer>(null);
    const [viewport, setViewport] = useState<Size>({ width: 700, height: 620 });
    const scene = useLabelStudioStore((state) => state.scene);
    const selectedId = useLabelStudioStore((state) => state.selectedId);
    const zoom = useLabelStudioStore((state) => state.zoom);
    const pan = useLabelStudioStore((state) => state.pan);
    const guidesVisible = useLabelStudioStore((state) => state.guidesVisible);
    const assets = useLabelStudioStore((state) => state.assets);
    const select = useLabelStudioStore((state) => state.select);
    const setZoom = useLabelStudioStore((state) => state.setZoom);
    const setPan = useLabelStudioStore((state) => state.setPan);
    const updateElement = useLabelStudioStore((state) => state.updateElement);
    const pixelsPerMm = mmToPx(1, zoom);
    const ruler = 28;
    const selectedElement = scene.elements.find(
      (element) => element.id === selectedId,
    );

    useEffect(() => {
      const host = hostRef.current;
      if (!host) return;
      const observer = new ResizeObserver(([entry]) => {
        if (!entry) return;
        setViewport({
          width: Math.max(320, Math.floor(entry.contentRect.width)),
          height: Math.max(320, Math.floor(entry.contentRect.height)),
        });
      });
      observer.observe(host);
      return () => observer.disconnect();
    }, []);

    useEffect(() => {
      const transformer = transformerRef.current;
      const stage = stageRef.current;
      if (!transformer || !stage) return;
      const node =
        selectedId && !selectedElement?.locked
          ? stage.findOne(`#${selectedId}`)
          : null;
      transformer.nodes(node ? [node] : []);
      transformer.forceUpdate();
      transformer.getLayer()?.batchDraw();
    }, [pan, scene, selectedElement?.locked, selectedId, zoom]);

    const rulerMarks = useMemo(() => {
      const marks: Array<{ value: number; x: number; y: number }> = [];
      for (let mm = 0; mm <= scene.physical.width_mm; mm += 10)
        marks.push({ value: mm, x: ruler + mm * pixelsPerMm, y: 0 });
      return marks;
    }, [pixelsPerMm, scene.physical.width_mm]);

    const dragEnd = (element: SceneElement, node: Konva.Node) => {
      const next = snapPosition(
        {
          x_mm: pxToMm(node.x() - ruler - pan.x, zoom),
          y_mm: pxToMm(node.y() - ruler - pan.y, zoom),
          width_mm: element.width_mm,
          height_mm: element.height_mm,
        },
        scene,
        pxToMm(6, zoom),
      );
      updateElement(element.id, next);
    };

    const transformEnd = (element: SceneElement, node: Konva.Node) => {
      const scaleX = node.scaleX();
      const scaleY = node.scaleY();
      node.scale({ x: 1, y: 1 });
      updateElement(element.id, {
        x_mm: pxToMm(node.x() - ruler - pan.x, zoom),
        y_mm: pxToMm(node.y() - ruler - pan.y, zoom),
        width_mm: Math.max(0.01, element.width_mm * scaleX),
        height_mm: Math.max(0.01, element.height_mm * scaleY),
        rotation_deg: node.rotation(),
        ...(element.type === 'text'
          ? {
              font_size_mm: Math.max(
                0.5,
                element.font_size_mm * Math.abs(scaleY),
              ),
            }
          : {}),
      });
    };

    const canvasWidth = scene.physical.width_mm * pixelsPerMm;
    const canvasHeight = scene.physical.height_mm * pixelsPerMm;
    const origin = { x: ruler + pan.x, y: ruler + pan.y };

    function renderStage(dpi: number) {
      const stage = stageRef.current;
      if (!stage) throw new Error('Label stage unavailable');
      const firstLayer = stage.getLayers()[0];
      const editorNodes = stage.find('.editor-only');
      firstLayer?.hide();
      editorNodes.forEach((node) => node.hide());
      try {
        const rendered = stage.toCanvas({
          x: origin.x,
          y: origin.y,
          width: canvasWidth,
          height: canvasHeight,
          pixelRatio: dpi / (PREVIEW_DPI * zoom),
        });
        const output = document.createElement('canvas');
        const dimensions = exportPixelDimensions(
          scene.physical.width_mm,
          scene.physical.height_mm,
          dpi,
        );
        output.width = dimensions.width;
        output.height = dimensions.height;
        const context = output.getContext('2d');
        if (!context) throw new Error('Canvas export unavailable');
        context.drawImage(rendered, 0, 0, output.width, output.height);
        const dataUrl = output.toDataURL('image/png');
        if (dataUrl.length > 90 * 1024 * 1024)
          throw new Error('Label export exceeds frontend limit');
        return dataUrl;
      } finally {
        firstLayer?.show();
        editorNodes.forEach((node) => node.show());
        stage.batchDraw();
      }
    }

    useImperativeHandle(forwardedRef, () => ({
      getStage: () => stageRef.current,
      renderExport: (dpi) => renderStage(dpi),
      renderThumbnail: () => {
        const longestMm = Math.max(
          scene.physical.width_mm,
          scene.physical.height_mm,
        );
        return renderStage((320 * 25.4) / longestMm);
      },
    }));

    return (
      <div className="label-canvas-host" ref={hostRef}>
        <Stage
          ref={stageRef}
          width={viewport.width}
          height={viewport.height}
          draggable
          x={0}
          y={0}
          onClick={(event) => {
            if (event.target === event.target.getStage()) select(null);
          }}
          onDragEnd={(event) => {
            if (event.target === event.target.getStage()) {
              setPan({
                x: pan.x + event.target.x(),
                y: pan.y + event.target.y(),
              });
              event.target.position({ x: 0, y: 0 });
            }
          }}
          onWheel={(event) => {
            event.evt.preventDefault();
            const next = zoom * (event.evt.deltaY > 0 ? 0.9 : 1.1);
            setZoom(next);
          }}
        >
          <Layer listening={false}>
            <Rect
              width={viewport.width}
              height={viewport.height}
              fill="#080b0e"
            />
            <Rect
              x={ruler}
              width={viewport.width - ruler}
              height={ruler}
              fill="#171c21"
            />
            <Rect
              y={ruler}
              width={ruler}
              height={viewport.height - ruler}
              fill="#171c21"
            />
            {rulerMarks.map((mark) => (
              <Group key={`x-${mark.value}`} x={mark.x + pan.x}>
                <Line points={[0, ruler - 8, 0, ruler]} stroke="#78828a" />
                <Text
                  x={2}
                  y={5}
                  text={String(mark.value)}
                  fontFamily="VT323"
                  fontSize={11}
                  fill="#a5adaf"
                />
              </Group>
            ))}
            {Array.from(
              { length: Math.floor(scene.physical.height_mm / 10) + 1 },
              (_, index) => index * 10,
            ).map((mm) => (
              <Group key={`y-${mm}`} y={origin.y + mm * pixelsPerMm}>
                <Line points={[ruler - 8, 0, ruler, 0]} stroke="#78828a" />
                <Text
                  x={2}
                  y={2}
                  text={String(mm)}
                  fontFamily="VT323"
                  fontSize={11}
                  fill="#a5adaf"
                />
              </Group>
            ))}
            <Text
              x={4}
              y={9}
              text="mm"
              fontFamily="VT323"
              fontSize={11}
              fill="#d29b50"
            />
            <Rect
              x={origin.x - 3}
              y={origin.y - 3}
              width={canvasWidth + 6}
              height={canvasHeight + 6}
              fill="#030405"
              shadowColor="#000"
              shadowBlur={16}
            />
          </Layer>
          <Layer>
            <Group
              clipFunc={
                scene.physical.shape.type === 'circle'
                  ? (context) => {
                      context.arc(
                        origin.x + canvasWidth / 2,
                        origin.y + canvasHeight / 2,
                        canvasWidth / 2,
                        0,
                        Math.PI * 2,
                      );
                    }
                  : undefined
              }
            >
              {scene.elements
                .filter((element) => element.visible)
                .sort((a, b) => a.z_order - b.z_order)
                .map((element) => (
                  <SceneNode
                    key={element.id}
                    element={element}
                    assets={assets}
                    origin={origin}
                    pixelsPerMm={pixelsPerMm}
                    selected={element.id === selectedId}
                    onSelect={() => !element.locked && select(element.id)}
                    onDragEnd={(node) => dragEnd(element, node)}
                    onTransformEnd={(node) => transformEnd(element, node)}
                    onCropChange={(crop) => updateElement(element.id, { crop })}
                  />
                ))}
            </Group>
            {guidesVisible && (
              <Group name="editor-only">
                <Rect
                  x={origin.x + scene.physical.safe_margin_mm * pixelsPerMm}
                  y={origin.y + scene.physical.safe_margin_mm * pixelsPerMm}
                  width={
                    canvasWidth -
                    scene.physical.safe_margin_mm * pixelsPerMm * 2
                  }
                  height={
                    canvasHeight -
                    scene.physical.safe_margin_mm * pixelsPerMm * 2
                  }
                  stroke="#38d991"
                  dash={[5, 4]}
                  listening={false}
                />
                <Line
                  points={[
                    origin.x + canvasWidth / 2,
                    origin.y,
                    origin.x + canvasWidth / 2,
                    origin.y + canvasHeight,
                  ]}
                  stroke="#d49442"
                  opacity={0.55}
                  listening={false}
                />
                <Line
                  points={[
                    origin.x,
                    origin.y + canvasHeight / 2,
                    origin.x + canvasWidth,
                    origin.y + canvasHeight / 2,
                  ]}
                  stroke="#d49442"
                  opacity={0.55}
                  listening={false}
                />
              </Group>
            )}
            {scene.physical.shape.type === 'circle' && (
              <Group name="editor-only">
                <Circle
                  x={origin.x + canvasWidth / 2}
                  y={origin.y + canvasHeight / 2}
                  radius={canvasWidth / 2}
                  stroke="#d49442"
                  listening={false}
                />
                <Circle
                  x={origin.x + canvasWidth / 2}
                  y={origin.y + canvasHeight / 2}
                  radius={
                    (scene.physical.shape.center_hole_mm / 2) * pixelsPerMm
                  }
                  fill="#080b0e"
                  stroke="#d49442"
                  listening={false}
                />
              </Group>
            )}
            <Transformer
              name="editor-only"
              ref={transformerRef}
              rotateEnabled
              flipEnabled={false}
              keepRatio={false}
              shiftBehavior="default"
              anchorStroke="#d49442"
              anchorFill="#111419"
              borderStroke="#38d991"
              boundBoxFunc={(oldBox, newBox) =>
                newBox.width < 4 || newBox.height < 4 ? oldBox : newBox
              }
            />
          </Layer>
        </Stage>
      </div>
    );
  },
);

function SceneNode({
  element,
  assets,
  origin,
  pixelsPerMm,
  selected,
  onSelect,
  onDragEnd,
  onTransformEnd,
  onCropChange,
}: {
  element: SceneElement;
  assets: ArtworkAsset[];
  origin: { x: number; y: number };
  pixelsPerMm: number;
  selected: boolean;
  onSelect: () => void;
  onDragEnd: (node: Konva.Node) => void;
  onTransformEnd: (node: Konva.Node) => void;
  onCropChange: (crop: NormalizedCrop) => void;
}) {
  const artwork =
    element.type === 'image' || element.type === 'logo'
      ? assets.find((asset) => asset.id === element.artwork_id)
      : undefined;
  const image = useArtworkImage(artwork);
  const common = {
    id: element.id,
    x: origin.x + element.x_mm * pixelsPerMm,
    y: origin.y + element.y_mm * pixelsPerMm,
    width: element.width_mm * pixelsPerMm,
    height: element.height_mm * pixelsPerMm,
    rotation: element.rotation_deg,
    opacity: element.opacity,
    draggable: !element.locked,
    onClick: onSelect,
    onTap: onSelect,
    onDragEnd: (event: Konva.KonvaEventObject<DragEvent>) =>
      onDragEnd(event.target),
    onTransformEnd: (event: Konva.KonvaEventObject<Event>) =>
      onTransformEnd(event.target),
    shadowColor: selected ? '#38d991' : undefined,
    shadowBlur: selected ? 4 : 0,
    listening: element.type !== 'background',
  };
  if (element.type === 'text')
    return (
      <Text
        {...common}
        text={element.text}
        fontFamily={element.font_family}
        fontSize={element.font_size_mm * pixelsPerMm}
        fill={element.color}
        align={element.align}
        verticalAlign="middle"
      />
    );
  if (element.type === 'rect' || element.type === 'background')
    return (
      <Rect
        {...common}
        fill={element.type === 'background' ? element.color : element.fill}
        stroke={
          element.type === 'rect' ? (element.stroke ?? undefined) : undefined
        }
        strokeWidth={
          element.type === 'rect' ? element.stroke_width_mm * pixelsPerMm : 0
        }
        cornerRadius={
          element.type === 'rect' ? element.corner_radius_mm * pixelsPerMm : 0
        }
      />
    );
  if (element.type === 'line')
    return (
      <Line
        {...common}
        points={[0, 0, common.width, common.height]}
        stroke={element.color}
        strokeWidth={element.stroke_width_mm * pixelsPerMm}
      />
    );
  return (
    <Group
      {...common}
      draggable={element.type === 'image' ? false : common.draggable}
      onDragEnd={element.type === 'image' ? undefined : common.onDragEnd}
      clipX={0}
      clipY={0}
      clipWidth={common.width}
      clipHeight={common.height}
    >
      <Rect
        width={common.width}
        height={common.height}
        fill="#252d34"
        stroke="#d49442"
      />
      {image ? (
        element.type === 'image' && element.fit === 'cover' ? (
          <CoverImage
            image={image}
            frameWidth={common.width}
            frameHeight={common.height}
            crop={element.crop}
            draggable={!element.locked}
            onSelect={onSelect}
            onCropChange={onCropChange}
          />
        ) : (
          <KonvaImage
            image={image}
            {...fitImage(
              image.width,
              image.height,
              common.width,
              common.height,
              element.fit,
            )}
          />
        )
      ) : (
        <Text
          width={common.width}
          height={common.height}
          text={`ARTWORK\n${element.artwork_id.slice(0, 8)}`}
          fill="#d9dedb"
          align="center"
          verticalAlign="middle"
          fontFamily="VT323"
          fontSize={12}
        />
      )}
    </Group>
  );
}

function CoverImage({
  image,
  frameWidth,
  frameHeight,
  crop,
  draggable,
  onSelect,
  onCropChange,
}: {
  image: HTMLImageElement;
  frameWidth: number;
  frameHeight: number;
  crop: NormalizedCrop | null;
  draggable: boolean;
  onSelect: () => void;
  onCropChange: (crop: NormalizedCrop) => void;
}) {
  const resolved = resolveCoverCrop(
    image.width,
    image.height,
    frameWidth,
    frameHeight,
    crop,
  );
  const width = frameWidth / resolved.width;
  const height = frameHeight / resolved.height;
  const x = -(resolved.x * width);
  const y = -(resolved.y * height);
  return (
    <KonvaImage
      image={image}
      x={x}
      y={y}
      width={width}
      height={height}
      draggable={draggable}
      onClick={onSelect}
      onTap={onSelect}
      onDragMove={(event) => {
        event.target.position({
          x: Math.min(0, Math.max(frameWidth - width, event.target.x())),
          y: Math.min(0, Math.max(frameHeight - height, event.target.y())),
        });
      }}
      onDragEnd={(event) => {
        onCropChange({
          ...resolved,
          x: clamp(-event.target.x() / width, 0, 1 - resolved.width),
          y: clamp(-event.target.y() / height, 0, 1 - resolved.height),
        });
      }}
    />
  );
}

function resolveCoverCrop(
  sourceWidth: number,
  sourceHeight: number,
  frameWidth: number,
  frameHeight: number,
  crop: NormalizedCrop | null,
) {
  const sourceAspect = sourceWidth / sourceHeight;
  const frameAspect = frameWidth / frameHeight;
  const base =
    sourceAspect > frameAspect
      ? {
          width: frameAspect / sourceAspect,
          height: 1,
        }
      : {
          width: 1,
          height: sourceAspect / frameAspect,
        };
  const zoom = crop
    ? Math.max(base.width / crop.width, base.height / crop.height, 1)
    : 1;
  const width = base.width / zoom;
  const height = base.height / zoom;
  const centerX = crop ? crop.x + crop.width / 2 : 0.5;
  const centerY = crop ? crop.y + crop.height / 2 : 0.5;
  return {
    x: clamp(centerX - width / 2, 0, 1 - width),
    y: clamp(centerY - height / 2, 0, 1 - height),
    width,
    height,
  };
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}

function fitImage(
  sourceWidth: number,
  sourceHeight: number,
  frameWidth: number,
  frameHeight: number,
  fit: 'contain' | 'cover' | 'stretch',
) {
  if (fit === 'stretch')
    return { x: 0, y: 0, width: frameWidth, height: frameHeight };
  const scale =
    fit === 'contain'
      ? Math.min(frameWidth / sourceWidth, frameHeight / sourceHeight)
      : Math.max(frameWidth / sourceWidth, frameHeight / sourceHeight);
  const width = sourceWidth * scale;
  const height = sourceHeight * scale;
  return {
    x: (frameWidth - width) / 2,
    y: (frameHeight - height) / 2,
    width,
    height,
  };
}

function useArtworkImage(asset?: ArtworkAsset) {
  const [loaded, setLoaded] = useState<{
    id: string;
    image: HTMLImageElement;
  }>();
  useEffect(() => {
    if (!asset?.bytes) return;
    let cancelled = false;
    const blob = new Blob([new Uint8Array(asset.bytes)], {
      type: asset.mime_type,
    });
    const reader = new FileReader();
    const next = new Image();
    next.onload = () => {
      if (!cancelled) setLoaded({ id: asset.id, image: next });
    };
    reader.onload = () => {
      if (!cancelled && typeof reader.result === 'string')
        next.src = reader.result;
    };
    reader.readAsDataURL(blob);
    return () => {
      cancelled = true;
      reader.abort();
    };
  }, [asset]);
  return loaded && loaded.id === asset?.id ? loaded.image : undefined;
}
