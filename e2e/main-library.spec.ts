import { expect, test } from '@playwright/test';

test.describe('main library administration', () => {
  test.use({ viewport: { width: 1024, height: 680 } });

  test('renders searchable library and reviewed executable form', async ({
    page,
  }) => {
    await page.goto('/?window=main');
    await expect(page.getByRole('main', { name: 'Meus Jogos' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'MEUS JOGOS' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(page.getByText('SUA COLEÇÃO AINDA ESTÁ VAZIA.')).toBeVisible();
    await expectNoExternalOverflow(page);
    await page.screenshot({
      path: 'e2e/screenshots/my-games-1024x680.png',
      animations: 'disabled',
    });

    await page.getByRole('tab', { name: 'ADICIONAR JOGOS' }).click();
    await expect(
      page.getByRole('main', { name: 'Adicionar jogos' }),
    ).toBeVisible();
    await expect(page.getByPlaceholder('Nome do jogo')).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'SCAN STEAM' }),
    ).toBeVisible();
    await expect(
      page.getByRole('dialog', { name: 'CADASTRAR EXECUTÁVEL LOCAL' }),
    ).not.toBeVisible();
    await page.getByRole('button', { name: '+ CADASTRAR LOCAL' }).click();
    await expect(
      page.getByText('OPÇÕES DE INICIALIZAÇÃO · OPCIONAL'),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'PROCURAR .EXE…' }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'ESCOLHER PASTA…' }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'ESCOLHER IMAGEM…' }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'RESOLVER SEM EXECUTAR' }),
    ).toBeVisible();
    await expect(
      page.getByRole('tabpanel', { name: 'Media Creator' }),
    ).not.toBeVisible();
    await page
      .locator('.manual-register')
      .getByRole('button', { name: 'Fechar cadastro' })
      .click();
    await expectNoExternalOverflow(page);
    await page.screenshot({
      path: 'e2e/screenshots/library-1024x680.png',
      animations: 'disabled',
    });

    const chassisGeometry = await page.evaluate(() => {
      const outline = document.querySelector<SVGSVGElement>(
        '.window-chassis__outline',
      );
      const polygon = outline?.querySelector('polygon');
      return {
        htmlBackground: getComputedStyle(document.documentElement)
          .backgroundColor,
        bodyBackground: getComputedStyle(document.body).backgroundColor,
        rootBackground: getComputedStyle(
          document.getElementById('root') as HTMLElement,
        ).backgroundColor,
        viewBox: outline
          ? [outline.viewBox.baseVal.width, outline.viewBox.baseVal.height]
          : null,
        firstPoint: polygon
          ? [
              Number(polygon.points[0].x.toFixed(1)),
              Number(polygon.points[0].y.toFixed(1)),
            ]
          : null,
      };
    });
    expect(chassisGeometry).toEqual({
      htmlBackground: 'rgba(0, 0, 0, 0)',
      bodyBackground: 'rgba(0, 0, 0, 0)',
      rootBackground: 'rgba(0, 0, 0, 0)',
      viewBox: [1024, 680],
      firstPoint: [18, 0.8],
    });

    const libraryGeometry = await page.evaluate(() => ({
      outlines: document.querySelectorAll('.main-shell [data-chamfer-outline]')
        .length,
      roundedPanels: Array.from(
        document.querySelectorAll('.mechanical-panel'),
      ).some((element) => getComputedStyle(element).borderRadius !== '0px'),
    }));
    expect(libraryGeometry.outlines).toBeGreaterThanOrEqual(3);
    expect(libraryGeometry.roundedPanels).toBe(false);

    await page.getByRole('tab', { name: 'MEDIA CREATOR' }).click();
    await expect(
      page.getByRole('tabpanel', { name: 'Media Creator' }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'SALVAR PERFIL E GERAR PREVIEW' }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'SALVAR PERFIL E GAME.INI…' }),
    ).toBeVisible();
    await expect(
      page.getByText('ID INTERNO DO PERFIL (PROFILE_ID)'),
    ).toBeVisible();
    await expect(page.getByText('CAPA DO JOGO')).toBeVisible();
    await expect(page.getByText(/RECORDER IMAPI/i)).not.toBeVisible();
    await expect(page.getByText(/UNIDADE PARA GRAVAÇÃO/i)).not.toBeVisible();
    await expect(page.getByPlaceholder('Nome do jogo')).not.toBeVisible();
    await expectNoExternalOverflow(page);
    await page.screenshot({
      path: 'e2e/screenshots/media-creator-1024x680.png',
      animations: 'disabled',
    });

    const creatorGeometry = await page.evaluate(() => ({
      outlines: document.querySelectorAll('.main-shell [data-chamfer-outline]')
        .length,
      roundedPanels: Array.from(
        document.querySelectorAll('.mechanical-panel'),
      ).some((element) => getComputedStyle(element).borderRadius !== '0px'),
    }));
    expect(creatorGeometry.outlines).toBeGreaterThanOrEqual(1);
    expect(creatorGeometry.roundedPanels).toBe(false);

    await page.getByRole('tab', { name: 'SETTINGS' }).click();
    await expect(page.getByRole('tabpanel', { name: 'Settings' })).toBeVisible();
    await expect(
      page.getByText('INICIAR O MEDIADECK COM O WINDOWS'),
    ).toBeVisible();
    await expect(
      page.getByText('MONITOR ONDE O LAUNCHER SERÁ ABERTO'),
    ).toBeVisible();
    await expect(
      page.getByRole('checkbox', {
        name: /INICIAR O MEDIADECK COM O WINDOWS/,
      }),
    ).not.toBeChecked();
    await expectNoExternalOverflow(page);
    await page.screenshot({
      path: 'e2e/screenshots/settings-1024x680.png',
      animations: 'disabled',
    });
  });
});

async function expectNoExternalOverflow(page: import('@playwright/test').Page) {
  const overflow = await page.evaluate(() => {
    const root = document.documentElement;
    const shell = document.querySelector('.main-shell');
    return {
      documentX: root.scrollWidth > root.clientWidth,
      documentY: root.scrollHeight > root.clientHeight,
      shellX: shell ? shell.scrollWidth > shell.clientWidth : true,
      shellY: shell ? shell.scrollHeight > shell.clientHeight : true,
    };
  });
  expect(overflow).toEqual({
    documentX: false,
    documentY: false,
    shellX: false,
    shellY: false,
  });
}
