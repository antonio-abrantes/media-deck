import { MainWindow } from '@/features/main/MainWindow';
import { RuntimeWindow } from '@/features/runtime/RuntimeWindow';
import { getWindowSurface, type WindowSurface } from '@/app/window-surface';

type AppProps = {
  surface?: WindowSurface;
};

export function App({ surface = getWindowSurface() }: AppProps) {
  return surface === 'runtime' ? <RuntimeWindow /> : <MainWindow />;
}
