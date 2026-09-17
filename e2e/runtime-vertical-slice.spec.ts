import { test, expect } from '@playwright/test';

/**
 * Vertical-slice E2E (WP-0611).
 * Run against `pnpm preview` or the Vite dev server with the runtime query.
 * Full Tauri desktop automation remains optional until CI hosts a signed build.
 */
test.describe('runtime vertical slice', () => {
  test('mostra chassi frameless e zonas do launcher', async ({ page }) => {
    await page.goto('/?window=runtime');
    await expect(
      page.getByRole('heading', { name: 'MEDIADECK' }),
    ).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'SYSTEM READY' }),
    ).toBeVisible();
    await expect(page.getByLabel('Capa do jogo')).toBeVisible();
    await expect(page.getByLabel('Status de hardware')).toBeVisible();
    await expect(page.getByLabel('Minimizar')).toBeVisible();
    await expect(page.getByLabel('Fechar')).toBeVisible();

    const geometry = await page.evaluate(() => ({
      width: document.documentElement.scrollWidth,
      height: document.documentElement.scrollHeight,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      surface: document.documentElement.dataset.surface,
      htmlBackground: getComputedStyle(document.documentElement)
        .backgroundColor,
      bodyBackground: getComputedStyle(document.body).backgroundColor,
      rootBackground: getComputedStyle(document.querySelector('#root')!)
        .backgroundColor,
    }));
    expect(geometry).toEqual({
      width: 865,
      height: 458,
      viewportWidth: 865,
      viewportHeight: 458,
      surface: 'runtime',
      htmlBackground: 'rgba(0, 0, 0, 0)',
      bodyBackground: 'rgba(0, 0, 0, 0)',
      rootBackground: 'rgba(0, 0, 0, 0)',
    });
  });

  test('preserva a composição canônica durante a sequência', async ({
    page,
  }) => {
    await page.goto('/?window=runtime&preview=prototype');
    await expect(
      page.getByRole('heading', { name: 'NEON RUNNER' }),
    ).toBeVisible();
    await expect(page.getByRole('progressbar')).toHaveAttribute(
      'aria-valuenow',
      '78',
    );
    await expect(page.getByText('1280 × 960 (4:3)')).toBeVisible();

    const regions = await page.evaluate(() => {
      const bounds = (selector: string) => {
        const rectangle = document
          .querySelector(selector)
          ?.getBoundingClientRect();
        return rectangle
          ? {
              x: Math.round(rectangle.x),
              y: Math.round(rectangle.y),
              width: Math.round(rectangle.width),
              height: Math.round(rectangle.height),
            }
          : null;
      };
      const clipPath = (selector: string, pseudo?: string) =>
        getComputedStyle(document.querySelector(selector)!, pseudo).clipPath;
      const outlinePoints = (selector: string) =>
        Array.from(
          document.querySelectorAll(`${selector} > .chamfer-outline polygon`),
        ).map((polygon) => polygon.getAttribute('points'));
      const logo = document.querySelector('.chassis-title')!;
      const logoStyle = getComputedStyle(logo);
      const logoBounds = logo.getBoundingClientRect();

      return {
        terminal: bounds('.boot-terminal'),
        footer: bounds('.runtime-layout__footer'),
        cover: bounds('.cover-frame'),
        corners: {
          outer: clipPath('.runtime-window'),
          terminal: [
            clipPath('.boot-terminal'),
            clipPath('.boot-terminal', '::before'),
            clipPath('.boot-terminal', '::after'),
          ],
          footer: [
            clipPath('.runtime-layout__footer'),
            clipPath('.runtime-layout__footer', '::before'),
            clipPath('.runtime-layout__footer', '::after'),
          ],
          cover: [
            clipPath('.cover-frame'),
            clipPath('.cover-frame', '::before'),
            clipPath('.cover-frame', '::after'),
          ],
        },
        outerEdge: {
          color: getComputedStyle(document.querySelector('.runtime-window')!)
            .backgroundColor,
          headerDecoration: getComputedStyle(
            document.querySelector('.chassis-header')!,
            '::after',
          ).content,
        },
        outlines: {
          outer: outlinePoints('.runtime-window'),
          terminal: outlinePoints('.boot-terminal'),
          footer: outlinePoints('.runtime-layout__footer'),
          cover: outlinePoints('.cover-frame'),
        },
        logo: {
          fontFamily: logoStyle.fontFamily,
          fontWeight: logoStyle.fontWeight,
          width: Math.round(logoBounds.width),
          height: Math.round(logoBounds.height),
        },
      };
    });

    expect(regions).toMatchObject({
      terminal: { x: 19, y: 58, width: 550, height: 326 },
      footer: { x: 19, y: 390, width: 550, height: 57 },
      cover: { x: 577, y: 58, width: 277, height: 389 },
    });
    expect(regions.corners.outer).toContain('18px 0px');
    expect(regions.corners.terminal[0]).toContain('10px 0px');
    expect(regions.corners.terminal[1]).toContain('9px 0px');
    expect(regions.corners.terminal[2]).toContain('7px 0px');
    expect(regions.corners.footer[0]).toContain('9px 0px');
    expect(regions.corners.footer[1]).toContain('8px 0px');
    expect(regions.corners.footer[2]).toContain('6px 0px');
    expect(regions.corners.cover[0]).toContain('10px 0px');
    expect(regions.corners.cover[1]).toContain('9px 0px');
    expect(regions.corners.cover[2]).toContain('7px 0px');
    expect(regions.outerEdge).toEqual({
      color: 'rgb(17, 20, 25)',
      headerDecoration: 'none',
    });
    expect(regions.outlines.outer).toHaveLength(1);
    expect(regions.outlines.outer[0]).toContain('18,0.8 847,0.8');
    expect(regions.outlines.terminal).toHaveLength(2);
    expect(regions.outlines.terminal[1]).toContain('10,4.5 540,4.5');
    expect(regions.outlines.footer).toHaveLength(2);
    expect(regions.outlines.footer[1]).toContain('9,4.5 541,4.5');
    expect(regions.outlines.cover).toHaveLength(2);
    expect(regions.outlines.cover[1]).toContain('10,8 267,8');
    expect(regions.logo.fontFamily).toContain('Orbitron MediaDeck');
    expect(regions.logo.fontWeight).toBe('700');
    expect(regions.logo.width).toBeGreaterThanOrEqual(230);
    expect(regions.logo.width).toBeLessThanOrEqual(245);
    expect(regions.logo.height).toBeGreaterThanOrEqual(22);

    await page.screenshot({
      path: 'docs/assets/screenshots/runtime-865x458.png',
    });
  });
});
