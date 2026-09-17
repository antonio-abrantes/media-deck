import type {
  LabelPresetKind,
  LabelScene,
  SceneElement,
  ToolKind,
} from './types';

export const MM_PER_INCH = 25.4;
export const PREVIEW_DPI = 96;
export const HISTORY_LIMIT = 50;

export const PRESETS: Record<
  LabelPresetKind,
  {
    label: string;
    width_mm: number;
    height_mm: number;
    shape: LabelScene['physical']['shape'];
    safe_margin_mm: number;
  }
> = {
  launcher_cover: {
    label: 'Capa do launcher · 1518 × 2076 px',
    width_mm: 128.524,
    height_mm: 175.768,
    shape: { type: 'rectangle' },
    safe_margin_mm: 7.62,
  },
  floppy_label: {
    label: 'Disquete 3,5" · 70 × 52 mm',
    width_mm: 70,
    height_mm: 52,
    shape: { type: 'rectangle' },
    safe_margin_mm: 3,
  },
  cd_jewel_front: {
    label: 'Jewel front · 120 × 120 mm',
    width_mm: 120,
    height_mm: 120,
    shape: { type: 'rectangle' },
    safe_margin_mm: 3,
  },
  cd_disc_label: {
    label: 'CD/DVD · Ø 120 mm',
    width_mm: 120,
    height_mm: 120,
    shape: { type: 'circle', center_hole_mm: 15 },
    safe_margin_mm: 3,
  },
  custom: {
    label: 'Tamanho personalizado',
    width_mm: 100,
    height_mm: 70,
    shape: { type: 'rectangle' },
    safe_margin_mm: 3,
  },
};

export function mmToPx(mm: number, zoom = 1, dpi = PREVIEW_DPI) {
  return (mm / MM_PER_INCH) * dpi * zoom;
}

export function pxToMm(px: number, zoom = 1, dpi = PREVIEW_DPI) {
  return (px / (dpi * zoom)) * MM_PER_INCH;
}

export function exportPixelDimensions(
  widthMm: number,
  heightMm: number,
  dpi: number,
) {
  return {
    width: Math.round(mmToPx(widthMm, 1, dpi)),
    height: Math.round(mmToPx(heightMm, 1, dpi)),
  };
}

export function createScene(
  kind: LabelPresetKind,
  custom?: { width_mm: number; height_mm: number },
): LabelScene {
  const preset = PRESETS[kind];
  const width_mm = custom?.width_mm ?? preset.width_mm;
  const height_mm = custom?.height_mm ?? preset.height_mm;
  const shape =
    kind === 'cd_disc_label'
      ? { ...preset.shape }
      : ({ type: 'rectangle' } as const);
  return {
    schema_version: 1,
    revision: 0,
    preset: { kind, version: 1 },
    physical: {
      width_mm,
      height_mm,
      bleed_mm: kind === 'launcher_cover' ? 0 : 3,
      safe_margin_mm: Math.min(
        preset.safe_margin_mm,
        width_mm / 3,
        height_mm / 3,
      ),
      dpi: 300,
      shape,
    },
    elements: [
      {
        type: 'background',
        id: 'background',
        x_mm: 0,
        y_mm: 0,
        width_mm,
        height_mm,
        rotation_deg: 0,
        opacity: 1,
        z_order: -1,
        locked: true,
        visible: true,
        color: '#F4F0E8FF',
      },
    ],
  };
}

function elementId(type: ToolKind) {
  return `${type}-${crypto.randomUUID().replaceAll('-', '').slice(0, 12)}`;
}

export function createElement(
  type: Exclude<ToolKind, 'background'>,
  scene: LabelScene,
  artworkId?: string,
): SceneElement | null {
  const width = Math.min(32, scene.physical.width_mm * 0.5);
  const height = Math.min(
    type === 'line' ? 0.5 : 18,
    scene.physical.height_mm * 0.4,
  );
  const frame = {
    id: elementId(type),
    x_mm: (scene.physical.width_mm - width) / 2,
    y_mm: (scene.physical.height_mm - height) / 2,
    width_mm: width,
    height_mm: height,
    rotation_deg: 0,
    opacity: 1,
    z_order: Math.max(0, ...scene.elements.map((item) => item.z_order + 1)),
    locked: false,
    visible: true,
  };
  switch (type) {
    case 'text':
      return {
        ...frame,
        type,
        text: 'NOVO TEXTO',
        font_family: 'Arial',
        font_size_mm: 5,
        color: '#111111FF',
        align: 'center',
      };
    case 'rect':
      return {
        ...frame,
        type,
        fill: '#D28A32FF',
        stroke: '#32291FFF',
        stroke_width_mm: 0.3,
        corner_radius_mm: 0,
      };
    case 'line':
      return { ...frame, type, color: '#111111FF', stroke_width_mm: 0.4 };
    case 'image':
      return artworkId
        ? {
            ...frame,
            x_mm: 0,
            y_mm: 0,
            width_mm: scene.physical.width_mm,
            height_mm: scene.physical.height_mm,
            type,
            artwork_id: artworkId,
            fit: 'cover',
            crop: null,
          }
        : null;
    case 'logo':
      return artworkId
        ? { ...frame, type, artwork_id: artworkId, fit: 'contain', tint: null }
        : null;
  }
}

export function normalizeZ(elements: SceneElement[]) {
  return [...elements]
    .sort((a, b) => a.z_order - b.z_order)
    .map((element, index) => ({
      ...element,
      z_order: element.type === 'background' ? -1 : index,
    }));
}

export function reorderElement(
  elements: SceneElement[],
  id: string,
  direction: -1 | 1,
) {
  const ordered = [...elements].sort((a, b) => a.z_order - b.z_order);
  const index = ordered.findIndex((element) => element.id === id);
  const target = index + direction;
  if (
    index < 0 ||
    target < 0 ||
    target >= ordered.length ||
    ordered[index]?.type === 'background' ||
    ordered[target]?.type === 'background'
  )
    return elements;
  [ordered[index], ordered[target]] = [ordered[target], ordered[index]];
  return normalizeZ(ordered);
}

export function snapPosition(
  value: { x_mm: number; y_mm: number; width_mm: number; height_mm: number },
  scene: LabelScene,
  thresholdMm: number,
) {
  const xGuides = [
    0,
    scene.physical.safe_margin_mm,
    scene.physical.width_mm / 2,
    scene.physical.width_mm - scene.physical.safe_margin_mm,
    scene.physical.width_mm,
  ];
  const yGuides = [
    0,
    scene.physical.safe_margin_mm,
    scene.physical.height_mm / 2,
    scene.physical.height_mm - scene.physical.safe_margin_mm,
    scene.physical.height_mm,
  ];
  const snapAxis = (
    position: number,
    size: number,
    guides: number[],
  ): number => {
    const anchors = [position, position + size / 2, position + size];
    for (const guide of guides) {
      for (let index = 0; index < anchors.length; index += 1) {
        if (Math.abs(anchors[index] - guide) <= thresholdMm) {
          return guide - [0, size / 2, size][index];
        }
      }
    }
    return position;
  };
  return {
    x_mm: snapAxis(value.x_mm, value.width_mm, xGuides),
    y_mm: snapAxis(value.y_mm, value.height_mm, yGuides),
  };
}

export function sceneValidation(scene: LabelScene) {
  const errors: string[] = [];
  const physical = scene.physical;
  const finiteRange = (value: number, min: number, max: number) =>
    Number.isFinite(value) && value >= min && value <= max;
  const color = (value: string) =>
    /^#[\dA-Fa-f]{6}([\dA-Fa-f]{2})?$/.test(value);
  if (scene.schema_version !== 1) errors.push('Versão de cena incompatível');
  if (
    !finiteRange(physical.width_mm, 0.1, 2000) ||
    !finiteRange(physical.height_mm, 0.1, 2000)
  )
    errors.push('Dimensões físicas inválidas');
  if (
    !finiteRange(physical.bleed_mm, 0, 50) ||
    !finiteRange(physical.safe_margin_mm, 0, 100) ||
    physical.safe_margin_mm * 2 >=
      Math.min(physical.width_mm, physical.height_mm)
  )
    errors.push('Sangria ou margem segura inválida');
  if (
    !Number.isInteger(physical.dpi) ||
    physical.dpi < 72 ||
    physical.dpi > 1200
  )
    errors.push('DPI inválido');
  if (
    scene.preset.kind === 'cd_disc_label' &&
    (physical.shape.type !== 'circle' ||
      physical.width_mm !== physical.height_mm ||
      !finiteRange(physical.shape.center_hole_mm, 0, physical.width_mm - 0.1))
  )
    errors.push('Geometria circular inválida');
  if (
    scene.preset.kind !== 'cd_disc_label' &&
    physical.shape.type !== 'rectangle'
  )
    errors.push('Shape incompatível com o preset');
  if (scene.elements.length > 512) errors.push('Limite de elementos excedido');
  if (
    new Set(scene.elements.map((item) => item.id)).size !==
    scene.elements.length
  )
    errors.push('IDs de elementos duplicados');
  if (
    new Set(scene.elements.map((item) => item.z_order)).size !==
    scene.elements.length
  )
    errors.push('Ordem de camadas duplicada');
  const backgrounds = scene.elements.filter(
    (element) => element.type === 'background',
  );
  if (backgrounds.length > 1) errors.push('Background duplicado');
  for (const element of scene.elements) {
    if (
      !/^[A-Za-z0-9_-]{1,64}$/.test(element.id) ||
      !finiteRange(element.x_mm, -10000, 10000) ||
      !finiteRange(element.y_mm, -10000, 10000) ||
      !finiteRange(element.width_mm, 0.01, 2000) ||
      !finiteRange(element.height_mm, 0.01, 2000) ||
      !finiteRange(element.rotation_deg, -360, 360) ||
      !finiteRange(element.opacity, 0, 1) ||
      !Number.isInteger(element.z_order) ||
      element.z_order < -10000 ||
      element.z_order > 10000
    ) {
      errors.push(`Frame inválido: ${element.id}`);
      continue;
    }
    if (
      element.type === 'background' &&
      (element.x_mm !== 0 ||
        element.y_mm !== 0 ||
        element.width_mm !== physical.width_mm ||
        element.height_mm !== physical.height_mm ||
        element.rotation_deg !== 0 ||
        !color(element.color))
    )
      errors.push('Background inválido');
    if (
      element.type === 'text' &&
      (new TextEncoder().encode(element.text).length > 16 * 1024 ||
        !element.font_family ||
        element.font_family.length > 100 ||
        !finiteRange(element.font_size_mm, 0.5, 500) ||
        !color(element.color))
    )
      errors.push(`Texto inválido: ${element.id}`);
    if (
      element.type === 'rect' &&
      (!color(element.fill) ||
        (element.stroke !== null && !color(element.stroke)) ||
        !finiteRange(element.stroke_width_mm, 0, 100) ||
        !finiteRange(element.corner_radius_mm, 0, 2000))
    )
      errors.push(`Retângulo inválido: ${element.id}`);
    if (
      element.type === 'line' &&
      (!color(element.color) ||
        !finiteRange(element.stroke_width_mm, 0.01, 100))
    )
      errors.push(`Linha inválida: ${element.id}`);
    if (
      element.type === 'logo' &&
      element.tint !== null &&
      !color(element.tint)
    )
      errors.push(`Logo inválido: ${element.id}`);
  }
  return errors;
}

export function cloneScene(scene: LabelScene): LabelScene {
  return structuredClone(scene);
}
