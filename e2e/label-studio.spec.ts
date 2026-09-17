import { expect, test } from '@playwright/test';

test.describe('Label Studio', () => {
  test.use({ viewport: { width: 1024, height: 680 } });

  test('edits physical presets in an exclusive tab workspace', async ({
    page,
  }) => {
    await page.goto('/?window=main');
    await page.getByRole('tab', { name: 'LABEL STUDIO' }).click();

    await expect(
      page.getByRole('tabpanel', { name: 'Label Studio' }),
    ).toBeVisible();
    await expect(page.getByPlaceholder('Nome do jogo')).not.toBeVisible();
    await expect(
      page.locator('.label-canvas-host canvas').first(),
    ).toBeVisible();
    await expect(page.getByText('70 × 52 mm', { exact: true })).toBeVisible();

    await page.getByRole('button', { name: 'TEXTO' }).click();
    await expect(page.getByText(/TEXT ·/)).toBeVisible();
    await expect(page.getByText('TEXT', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'EXCLUIR ELEMENTO' }).click();
    await expect(
      page.getByRole('alertdialog', { name: 'EXCLUIR ELEMENTO?' }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'CANCELAR' }).click();
    await expect(page.getByText('TEXT', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'EXCLUIR ELEMENTO' }).click();
    await page.getByRole('button', { name: 'CONFIRMAR EXCLUSÃO' }).click();
    await expect(page.getByText('TEXT', { exact: true })).not.toBeVisible();

    await page.getByRole('button', { name: 'CD/DVD · Ø 120 mm' }).click();
    await expect(page.getByText('120 × 120 mm', { exact: true })).toBeVisible();
    await expect(page.getByText('FURO CENTRAL MM')).toBeVisible();
    await page.getByRole('button', { name: 'COLAR IMAGEM' }).click();
    await expect(page.getByText(/área de transferência/)).toBeVisible();
    await expect(page.getByRole('button', { name: 'EXPORTAR' })).toBeEnabled();
    await expect(
      page.getByRole('button', { name: 'CALIBRAR IMPRESSÃO' }),
    ).toBeEnabled();
    await page.getByRole('button', { name: 'CALIBRAR IMPRESSÃO' }).click();
    await expect(
      page.getByText('Não foi possível gerar a folha de calibração.'),
    ).toBeVisible();

    const geometry = await page.evaluate(() => ({
      outlines: document.querySelectorAll(
        '.label-studio [data-chamfer-outline]',
      ).length,
      rounded: Array.from(
        document.querySelectorAll('.label-studio .mechanical-panel'),
      ).some((element) => getComputedStyle(element).borderRadius !== '0px'),
    }));
    expect(geometry.outlines).toBeGreaterThanOrEqual(5);
    expect(geometry.rounded).toBe(false);

    const overflow = await page.evaluate(() => {
      const root = document.documentElement;
      const studio = document.querySelector('.label-studio');
      return {
        documentX: root.scrollWidth > root.clientWidth,
        documentY: root.scrollHeight > root.clientHeight,
        studioX: studio ? studio.scrollWidth > studio.clientWidth : true,
        studioY: studio ? studio.scrollHeight > studio.clientHeight : true,
      };
    });
    expect(overflow).toEqual({
      documentX: false,
      documentY: false,
      studioX: false,
      studioY: false,
    });
    await page.screenshot({
      path: 'e2e/screenshots/label-studio-1024x680.png',
      animations: 'disabled',
    });
  });
});
