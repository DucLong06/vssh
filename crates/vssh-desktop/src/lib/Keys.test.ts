import { render } from '@testing-library/svelte'
import { expect, test } from 'vitest'
import Keys from './Keys.svelte'

const caps = (keys: string) => {
  const { container } = render(Keys, { keys })
  const key = container.querySelector('kbd')!
  return { text: key.textContent, caps: [...key.querySelectorAll('span:not(.sr-only)')].map((cap) => cap.textContent) }
}

test('a key is one cap; a chord is a cap for each of its keys, and reads as it is written', () => {
  expect(caps('Esc')).toEqual({ text: 'Esc', caps: ['Esc'] })
  expect(caps('Ctrl+K')).toEqual({ text: 'Ctrl+K', caps: ['Ctrl', 'K'] })
  expect(caps('Ctrl+Shift+Tab')).toEqual({ text: 'Ctrl+Shift+Tab', caps: ['Ctrl', 'Shift', 'Tab'] })
  expect(caps('Ctrl+/')).toEqual({ text: 'Ctrl+/', caps: ['Ctrl', '/'] })
  // What is not a chord is one cap, whatever is in it.
  expect(caps('0–9')).toEqual({ text: '0–9', caps: ['0–9'] })
  expect(caps('?')).toEqual({ text: '?', caps: ['?'] })
})
