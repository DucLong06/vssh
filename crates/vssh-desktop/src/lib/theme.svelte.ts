// The window's theme: TokyoNight Night, or Day. The colours themselves are
// in app.css; this says which block of it holds.
export type Theme = 'night' | 'day'

const saved = (): Theme => (localStorage.getItem('vssh.theme') === 'day' ? 'day' : 'night')

export const theme = $state({ name: saved() })

export function setTheme(name: Theme) {
  theme.name = name
  localStorage.setItem('vssh.theme', name)
  document.documentElement.dataset.theme = name
}

setTheme(theme.name)
