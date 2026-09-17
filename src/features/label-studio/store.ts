import { create } from 'zustand';
import {
  cloneScene,
  createElement,
  createScene,
  HISTORY_LIMIT,
  normalizeZ,
  reorderElement,
} from './engine';
import type {
  ArtworkAsset,
  ElementFrame,
  LabelPresetKind,
  LabelProject,
  LabelScene,
  NormalizedCrop,
} from './types';

type ScenePatch = Partial<
  ElementFrame & {
    text: string;
    font_family: string;
    font_size_mm: number;
    color: string;
    align: 'left' | 'center' | 'right';
    fill: string;
    stroke: string | null;
    stroke_width_mm: number;
    corner_radius_mm: number;
    artwork_id: string;
    fit: 'contain' | 'cover' | 'stretch';
    crop: NormalizedCrop | null;
    tint: string | null;
  }
>;

type LabelStudioState = {
  scene: LabelScene;
  projectId: string | null;
  projectName: string;
  gameId: string | null;
  projects: LabelProject[];
  assets: ArtworkAsset[];
  selectedId: string | null;
  zoom: number;
  pan: { x: number; y: number };
  guidesVisible: boolean;
  past: LabelScene[];
  future: LabelScene[];
  lastMutationKey: string | null;
  lastMutationAt: number;
  dirty: boolean;
  setProjects: (projects: LabelProject[]) => void;
  setAssets: (assets: ArtworkAsset[]) => void;
  cacheAsset: (asset: ArtworkAsset) => void;
  setGameId: (gameId: string | null) => void;
  select: (id: string | null) => void;
  setZoom: (zoom: number) => void;
  setPan: (pan: { x: number; y: number }) => void;
  toggleGuides: () => void;
  setProjectName: (name: string) => void;
  loadProject: (project: LabelProject) => void;
  markSaved: (project: LabelProject) => void;
  newProject: (
    preset: LabelPresetKind,
    custom?: { width_mm: number; height_mm: number },
  ) => void;
  addElement: (
    type: 'text' | 'rect' | 'line' | 'image' | 'logo',
    artworkId?: string,
  ) => void;
  updateElement: (id: string, patch: ScenePatch) => void;
  updatePhysical: (patch: Partial<LabelScene['physical']>) => void;
  removeSelected: () => void;
  toggleLock: (id: string) => void;
  toggleVisibility: (id: string) => void;
  reorder: (id: string, direction: -1 | 1) => void;
  undo: () => void;
  redo: () => void;
  reset: () => void;
};

const initialScene = createScene('floppy_label');

const initialState = {
  scene: initialScene,
  projectId: null,
  projectName: 'Etiqueta sem título',
  gameId: null,
  projects: [] as LabelProject[],
  assets: [] as ArtworkAsset[],
  selectedId: null as string | null,
  zoom: 1,
  pan: { x: 0, y: 0 },
  guidesVisible: true,
  past: [] as LabelScene[],
  future: [] as LabelScene[],
  lastMutationKey: null as string | null,
  lastMutationAt: 0,
  dirty: false,
};

function commit(
  state: LabelStudioState,
  scene: LabelScene,
  selectedId = state.selectedId,
  mutationKey?: string,
) {
  const now = Date.now();
  const coalesce =
    Boolean(mutationKey) &&
    mutationKey === state.lastMutationKey &&
    now - state.lastMutationAt <= 1000;
  return {
    scene,
    selectedId,
    past: coalesce
      ? state.past
      : [...state.past.slice(-(HISTORY_LIMIT - 1)), cloneScene(state.scene)],
    future: [],
    lastMutationKey: mutationKey ?? null,
    lastMutationAt: mutationKey ? now : 0,
    dirty: true,
  };
}

export const useLabelStudioStore = create<LabelStudioState>((set) => ({
  ...initialState,
  setProjects: (projects) => set({ projects }),
  setAssets: (assets) =>
    set((state) => ({
      assets: [
        ...state.assets.filter((asset) => asset.game_id === ''),
        ...assets,
      ],
    })),
  cacheAsset: (asset) =>
    set((state) => ({
      assets: [asset, ...state.assets.filter((item) => item.id !== asset.id)],
    })),
  setGameId: (gameId) => set({ gameId }),
  select: (selectedId) => set({ selectedId }),
  setZoom: (zoom) => set({ zoom: Math.max(0.25, Math.min(4, zoom)) }),
  setPan: (pan) => set({ pan }),
  toggleGuides: () => set((state) => ({ guidesVisible: !state.guidesVisible })),
  setProjectName: (projectName) => set({ projectName, dirty: true }),
  loadProject: (project) =>
    set({
      scene: cloneScene(project.scene),
      projectId: project.id,
      projectName: project.name,
      gameId: project.game_id,
      selectedId: null,
      past: [],
      future: [],
      lastMutationKey: null,
      lastMutationAt: 0,
      dirty: false,
    }),
  markSaved: (project) =>
    set((state) => ({
      scene: cloneScene(project.scene),
      projectId: project.id,
      projectName: project.name,
      gameId: project.game_id,
      projects: [
        project,
        ...state.projects.filter((item) => item.id !== project.id),
      ],
      past: [],
      future: [],
      lastMutationKey: null,
      lastMutationAt: 0,
      dirty: false,
    })),
  newProject: (preset, custom) =>
    set({
      ...initialState,
      scene: createScene(preset, custom),
      projectName:
        preset === 'custom' ? 'Projeto personalizado' : 'Novo projeto',
    }),
  addElement: (type, artworkId) =>
    set((state) => {
      const element = createElement(type, state.scene, artworkId);
      if (!element) return {};
      return commit(
        state,
        {
          ...state.scene,
          elements: normalizeZ([...state.scene.elements, element]),
        },
        element.id,
      );
    }),
  updateElement: (id, patch) =>
    set((state) => {
      const elements = state.scene.elements.map((element) =>
        element.id === id &&
        (!element.locked ||
          (element.type === 'background' &&
            Object.keys(patch).every((key) => key === 'color')))
          ? { ...element, ...patch }
          : element,
      );
      if (
        elements.every(
          (element, index) => element === state.scene.elements[index],
        )
      )
        return {};
      return commit(
        state,
        { ...state.scene, elements },
        state.selectedId,
        `element:${id}:${Object.keys(patch).sort().join(',')}`,
      );
    }),
  updatePhysical: (patch) =>
    set((state) => {
      const physical = { ...state.scene.physical, ...patch };
      const elements = state.scene.elements.map((element) =>
        element.type === 'background'
          ? {
              ...element,
              width_mm: physical.width_mm,
              height_mm: physical.height_mm,
            }
          : element,
      );
      return commit(
        state,
        { ...state.scene, physical, elements },
        state.selectedId,
        `physical:${Object.keys(patch).sort().join(',')}`,
      );
    }),
  removeSelected: () =>
    set((state) => {
      if (!state.selectedId) return {};
      const selected = state.scene.elements.find(
        (element) => element.id === state.selectedId,
      );
      if (!selected || selected.locked || selected.type === 'background')
        return {};
      return commit(
        state,
        {
          ...state.scene,
          elements: normalizeZ(
            state.scene.elements.filter(
              (element) => element.id !== state.selectedId,
            ),
          ),
        },
        null,
      );
    }),
  toggleLock: (id) =>
    set((state) => {
      const target = state.scene.elements.find((element) => element.id === id);
      if (!target || target.type === 'background') return {};
      return commit(state, {
        ...state.scene,
        elements: state.scene.elements.map((element) =>
          element.id === id ? { ...element, locked: !element.locked } : element,
        ),
      });
    }),
  toggleVisibility: (id) =>
    set((state) =>
      commit(state, {
        ...state.scene,
        elements: state.scene.elements.map((element) =>
          element.id === id
            ? { ...element, visible: !element.visible }
            : element,
        ),
      }),
    ),
  reorder: (id, direction) =>
    set((state) =>
      commit(state, {
        ...state.scene,
        elements: reorderElement(state.scene.elements, id, direction),
      }),
    ),
  undo: () =>
    set((state) => {
      const previous = state.past.at(-1);
      if (!previous) return {};
      return {
        scene: cloneScene(previous),
        past: state.past.slice(0, -1),
        future: [cloneScene(state.scene), ...state.future].slice(
          0,
          HISTORY_LIMIT,
        ),
        selectedId: null,
        lastMutationKey: null,
        lastMutationAt: 0,
        dirty: true,
      };
    }),
  redo: () =>
    set((state) => {
      const next = state.future[0];
      if (!next) return {};
      return {
        scene: cloneScene(next),
        past: [...state.past, cloneScene(state.scene)].slice(-HISTORY_LIMIT),
        future: state.future.slice(1),
        selectedId: null,
        lastMutationKey: null,
        lastMutationAt: 0,
        dirty: true,
      };
    }),
  reset: () => set({ ...initialState, scene: createScene('floppy_label') }),
}));

export function selectedElement() {
  const state = useLabelStudioStore.getState();
  return (
    state.scene.elements.find((element) => element.id === state.selectedId) ??
    null
  );
}
