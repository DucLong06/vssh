// The window's words. `strings.json` is the core's table
// (`vssh_core::i18n`), written out by the shell's tests: edit the table,
// not the file.
import type { Lang } from './api'
import strings from './strings.json'

export type Key = keyof typeof strings.en

const saved = (): Lang => (localStorage.getItem('vssh.lang') === 'vi' ? 'vi' : 'en')

export const i18n = $state({ lang: saved() })

export const t = (key: Key): string => strings[i18n.lang][key]

export function setLang(lang: Lang) {
  i18n.lang = lang
  localStorage.setItem('vssh.lang', lang)
  document.documentElement.lang = lang
}

setLang(i18n.lang)
