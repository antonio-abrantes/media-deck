import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from '@/app/App';
import { __resetSessionStoreForTests } from '@/features/runtime/store/session-store';
import { IDLE_SNAPSHOT } from '@/features/runtime/types';

describe('App', () => {
  beforeEach(() => {
    __resetSessionStoreForTests(IDLE_SNAPSHOT);
  });

  it('renderiza a janela administrativa no chassi compartilhado', () => {
    render(<App surface="main" />);

    expect(
      screen.getByRole('heading', { name: 'MEDIADECK' }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('Minimizar')).toBeInTheDocument();
    expect(screen.getByLabelText('Fechar')).toBeInTheDocument();
  });

  it('renderiza o runtime com zonas do launcher', () => {
    render(<App surface="runtime" />);

    expect(
      screen.getByRole('heading', { name: 'SYSTEM READY' }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('Capa do jogo')).toBeInTheDocument();
    expect(screen.getByText('MEDIADECK ARCHIVE')).toBeInTheDocument();
    expect(screen.getByLabelText('Status de hardware')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Simular inserção/i }),
    ).toBeInTheDocument();
  });
});
