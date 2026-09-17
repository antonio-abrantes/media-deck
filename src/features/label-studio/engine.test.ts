import { beforeEach, describe, expect, it } from 'vitest';
import {
  createScene,
  exportPixelDimensions,
  HISTORY_LIMIT,
  mmToPx,
  pxToMm,
  sceneValidation,
  snapPosition,
} from './engine';
import { useLabelStudioStore } from './store';

describe('Label Studio engine', () => {
  beforeEach(() => useLabelStudioStore.getState().reset());

  it('keeps millimetres canonical across preview scales', () => {
    for (const zoom of [0.25, 1, 2, 4]) {
      expect(pxToMm(mmToPx(120, zoom), zoom)).toBeCloseTo(120, 8);
    }
  });

  it('rounds physical export pixels consistently at print DPIs', () => {
    expect(
      [96, 150, 300, 600].map((dpi) => exportPixelDimensions(120, 120, dpi)),
    ).toEqual([
      { width: 454, height: 454 },
      { width: 709, height: 709 },
      { width: 1417, height: 1417 },
      { width: 2835, height: 2835 },
    ]);
  });

  it('exports the digital launcher cover at its canonical aperture ratio', () => {
    const scene = createScene('launcher_cover');
    expect(
      exportPixelDimensions(
        scene.physical.width_mm,
        scene.physical.height_mm,
        scene.physical.dpi,
      ),
    ).toEqual({ width: 1518, height: 2076 });
    expect(scene.physical.bleed_mm).toBe(0);
  });

  it('keeps the scene immutable while deriving export dimensions', () => {
    const scene = createScene('cd_jewel_front');
    const before = structuredClone(scene);
    exportPixelDimensions(
      scene.physical.width_mm,
      scene.physical.height_mm,
      scene.physical.dpi,
    );
    expect(scene).toEqual(before);
  });

  it('creates editable physical presets with valid scene v1 contracts', () => {
    const floppy = createScene('floppy_label');
    const jewel = createScene('cd_jewel_front');
    const disc = createScene('cd_disc_label');
    const custom = createScene('custom', { width_mm: 88, height_mm: 42 });

    expect(floppy.physical).toMatchObject({ width_mm: 70, height_mm: 52 });
    expect(jewel.physical).toMatchObject({ width_mm: 120, height_mm: 120 });
    expect(disc.physical.shape).toEqual({
      type: 'circle',
      center_hole_mm: 15,
    });
    expect(custom.physical).toMatchObject({ width_mm: 88, height_mm: 42 });
    expect([floppy, jewel, disc, custom].flatMap(sceneValidation)).toEqual([]);
  });

  it('snaps edges and centres to physical guides', () => {
    const scene = createScene('cd_jewel_front');
    expect(
      snapPosition(
        { x_mm: 44.8, y_mm: 2.7, width_mm: 30, height_mm: 20 },
        scene,
        0.4,
      ),
    ).toEqual({ x_mm: 45, y_mm: 3 });
  });

  it('reports values rejected by the Rust scene contract', () => {
    const scene = createScene('floppy_label');
    scene.physical.safe_margin_mm = 30;
    scene.elements[0].opacity = Number.NaN;
    expect(sceneValidation(scene)).toEqual(
      expect.arrayContaining([
        'Sangria ou margem segura inválida',
        'Frame inválido: background',
      ]),
    );
  });

  it('tracks bounded undo/redo independently from persisted revision', () => {
    const store = useLabelStudioStore.getState();
    store.addElement('text');
    const textId = useLabelStudioStore.getState().selectedId!;
    for (let index = 0; index < HISTORY_LIMIT + 5; index += 1) {
      useLabelStudioStore
        .getState()
        .updateElement(
          textId,
          index % 2 ? { x_mm: index + 0.5 } : { y_mm: index + 0.5 },
        );
    }
    const edited = useLabelStudioStore.getState();
    expect(edited.past).toHaveLength(HISTORY_LIMIT);
    expect(edited.scene.revision).toBe(0);
    edited.undo();
    expect(useLabelStudioStore.getState().future).toHaveLength(1);
    useLabelStudioStore.getState().redo();
    expect(useLabelStudioStore.getState().scene.revision).toBe(0);
  });

  it('coalesces continuous edits to the same property', () => {
    const store = useLabelStudioStore.getState();
    store.addElement('text');
    const textId = useLabelStudioStore.getState().selectedId!;
    const baseline = useLabelStudioStore.getState().past.length;
    for (let index = 0; index < 20; index += 1)
      useLabelStudioStore.getState().updateElement(textId, { x_mm: index });
    expect(useLabelStudioStore.getState().past).toHaveLength(baseline + 1);
  });

  it('keeps the saved revision stable through later undo', () => {
    const store = useLabelStudioStore.getState();
    store.addElement('text');
    const savedScene = structuredClone(useLabelStudioStore.getState().scene);
    savedScene.revision = 4;
    store.markSaved({
      id: '01994a56-69d7-7ef4-a137-94808fa24130',
      game_id: null,
      name: 'Revisionado',
      scene: savedScene,
      thumbnail: null,
      is_template: false,
      created_at: '2026-09-16T00:00:00Z',
      updated_at: '2026-09-16T00:00:00Z',
    });
    const textId = useLabelStudioStore.getState().scene.elements[1].id;
    useLabelStudioStore.getState().updateElement(textId, { x_mm: 12 });
    useLabelStudioStore.getState().undo();
    expect(useLabelStudioStore.getState().scene.revision).toBe(4);
  });

  it('keeps artwork references typed and never accepts paths', () => {
    const store = useLabelStudioStore.getState();
    store.addElement('image', '01994a56-69d7-7ef4-a137-94808fa24131');
    const image = useLabelStudioStore
      .getState()
      .scene.elements.find((element) => element.type === 'image');
    expect(image).toMatchObject({
      type: 'image',
      artwork_id: '01994a56-69d7-7ef4-a137-94808fa24131',
      x_mm: 0,
      y_mm: 0,
      width_mm: 70,
      height_mm: 52,
      fit: 'cover',
    });
    expect(image).not.toHaveProperty('path');
    if (!image) throw new Error('image element was not created');
    store.updateElement(image.id, {
      crop: { x: 0.1, y: 0.2, width: 0.6, height: 0.7 },
    });
    expect(
      useLabelStudioStore
        .getState()
        .scene.elements.find((element) => element.id === image.id),
    ).toMatchObject({
      crop: { x: 0.1, y: 0.2, width: 0.6, height: 0.7 },
    });
  });
});
