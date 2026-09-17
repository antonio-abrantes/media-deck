import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from '@/app/App';
import { getWindowSurface } from '@/app/window-surface';
import '@/styles/global.css';

const root = document.getElementById('root');
const surface = getWindowSurface();

document.documentElement.dataset.surface = surface;

if (!root) {
  throw new Error('Elemento raiz da aplicação não encontrado.');
}

createRoot(root).render(
  <StrictMode>
    <App surface={surface} />
  </StrictMode>,
);
