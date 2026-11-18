import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { setLang } from './lib/i18n.svelte'
import { setStreamer } from './lib/streamer.svelte'
import { setTheme } from './lib/theme.svelte'

// jsdom has no modal dialogs: open and close them the way a webview does.
HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true
}
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false
  this.dispatchEvent(new Event('close'))
}
Element.prototype.scrollIntoView ??= () => {}
// Nor anything under a point: a test that needs something there says what.
document.elementFromPoint ??= () => null
// Nor a layout to observe.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
}

afterEach(() => {
  setLang('en')
  setStreamer(false)
  setTheme('night')
})
