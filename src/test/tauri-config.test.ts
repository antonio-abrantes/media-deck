import { describe, expect, it } from 'vitest';
import mainCapability from '../../src-tauri/capabilities/main.json';
import runtimeCapability from '../../src-tauri/capabilities/runtime.json';
import tauriConfig from '../../src-tauri/tauri.conf.json';

const windowPermissions = [
  'core:window:allow-start-dragging',
  'core:window:allow-minimize',
  'core:window:allow-close',
  'core:event:allow-listen',
  'core:event:allow-unlisten',
] as const;

describe('Tauri security baseline', () => {
  it('declara main e runtime frameless sem fullscreen', () => {
    const windows = tauriConfig.app.windows;
    const runtime = windows.find((window) => window.label === 'runtime');
    const main = windows.find((window) => window.label === 'main');

    expect(windows.map(({ label }) => label)).toEqual(['main', 'runtime']);
    expect(windows.every(({ fullscreen }) => fullscreen === false)).toBe(true);
    expect(main?.decorations).toBe(false);
    expect(main?.transparent).toBe(true);
    expect(main?.shadow).toBe(false);
    expect(main?.width).toBe(1100);
    expect(main?.height).toBe(700);
    expect(main?.minWidth).toBe(900);
    expect(main?.minHeight).toBe(560);
    expect(runtime?.decorations).toBe(false);
    expect(runtime?.width).toBe(865);
    expect(runtime?.height).toBe(458);
    expect(runtime?.minWidth).toBe(865);
    expect(runtime?.maxWidth).toBe(865);
    expect(runtime?.minHeight).toBe(458);
    expect(runtime?.maxHeight).toBe(458);
    expect(runtime?.resizable).toBe(false);
    expect(runtime?.transparent).toBe(true);
    expect(runtime?.shadow).toBe(false);
  });

  it('Concede permissões mínimas de janela, eventos e comandos tipados', () => {
    expect(mainCapability.windows).toEqual(['main']);
    expect(runtimeCapability.windows).toEqual(['runtime']);
    expect(mainCapability.permissions).toEqual([
      ...windowPermissions,
      'dialog:allow-open',
      'dialog:allow-save',
      'main-admin',
    ]);
    expect(runtimeCapability.permissions).toEqual([
      ...windowPermissions,
      'runtime-session',
    ]);
    expect(mainCapability.permissions).not.toContain('core:default');
    expect(runtimeCapability.permissions).not.toContain('core:default');
  });

  it('bloqueia objetos, frames e formulários na CSP de release', () => {
    const { csp } = tauriConfig.app.security;

    expect(csp).toContain("img-src 'self' data: blob:");
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("frame-ancestors 'none'");
    expect(csp).toContain("form-action 'none'");
    expect(csp).toContain("font-src 'self'");
  });
});
