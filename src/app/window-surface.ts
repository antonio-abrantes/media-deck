export type WindowSurface = 'main' | 'runtime';

export function getWindowSurface(
  search = window.location.search,
): WindowSurface {
  return new URLSearchParams(search).get('window') === 'runtime'
    ? 'runtime'
    : 'main';
}
