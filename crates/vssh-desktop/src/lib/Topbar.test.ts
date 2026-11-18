import { render, screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'
import Topbar from './Topbar.svelte'

test('the head of the dashboard: its name as the heading and a line about it, and nothing to press', () => {
  render(Topbar, { title: 'Status', sub: '4 Hosts · 3 reachable · 1 down' })
  expect(screen.getByRole('heading', { level: 1, name: 'Status' })).toHaveClass('text-2xl', 'font-semibold')
  expect(screen.getByRole('banner')).toHaveTextContent('4 Hosts · 3 reachable · 1 down')
  // The palette, the language, streamer mode and the cheatsheet are the rail's now.
  expect(screen.queryAllByRole('button')).toHaveLength(0)
})
