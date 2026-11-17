import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { setLang } from './lib/i18n.svelte'

// jsdom has no modal dialogs: open and close them the way a webview does.
HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true
}
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false
  this.dispatchEvent(new Event('close'))
}
Element.prototype.scrollIntoView ??= () => {}

afterEach(() => setLang('en'))
