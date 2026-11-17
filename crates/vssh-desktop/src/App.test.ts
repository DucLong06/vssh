import { render, screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'
import App from './App.svelte'

test('the app mounts', () => {
  render(App)
  expect(screen.getByRole('heading', { name: 'vssh' })).toBeInTheDocument()
})
