import { expect, test, vi } from 'vitest'
import { scale, scaled, setScale, STEPS, zoom } from './scale.svelte'

const root = () => document.documentElement.style.fontSize

test('the window is made at 100: nothing kept, or what is kept is no number', async () => {
  expect(scale.percent).toBe(100)
  expect(root()).toBe('100%')
  localStorage.setItem('vssh.ui_scale', 'big')
  vi.resetModules()
  expect((await import('./scale.svelte')).scale.percent).toBe(100)
})

test('a size is one of the steps: what is asked for is the step nearest to it', () => {
  expect(STEPS).toEqual([80, 90, 100, 110, 125, 150])
  for (const [asked, got] of [[80, 80], [125, 125], [10, 80], [84, 80], [86, 90], [104, 100], [120, 125], [140, 150], [900, 150], [NaN, 100]]) {
    setScale(asked)
    expect(scale.percent, String(asked)).toBe(got)
  }
})

test('a step bigger, a step smaller; at either end it stays', () => {
  const walk = (by: 1 | -1, times: number) => Array.from({ length: times }, () => (zoom(by), scale.percent))
  expect(walk(1, 4)).toEqual([110, 125, 150, 150])
  expect(walk(-1, 6)).toEqual([125, 110, 100, 90, 80, 80])
})

test('the size is the root\'s letter, is kept, and is the one the window opens with the next time', async () => {
  setScale(125)
  expect(root()).toBe('125%')
  expect(localStorage.getItem('vssh.ui_scale')).toBe('125')
  expect(scaled(288)).toBe(360)
  vi.resetModules()
  expect((await import('./scale.svelte')).scale.percent).toBe(125)
  // What was kept by another hand and is between two steps: the nearest.
  localStorage.setItem('vssh.ui_scale', '117')
  vi.resetModules()
  expect((await import('./scale.svelte')).scale.percent).toBe(110)
})
