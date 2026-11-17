import { render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import QuickConnect from './QuickConnect.svelte'

function open(onconnect = vi.fn(async (_target: string) => {})) {
  const onclose = vi.fn()
  render(QuickConnect, { onconnect, onclose })
  const field = (name: string) => screen.getByLabelText(name)
  return { onconnect, onclose, user: userEvent.setup(), field }
}
const connect = () => screen.getByRole('button', { name: 'Connect' })

test('a host alone is enough', async () => {
  const { user, field, onconnect } = open()
  expect(screen.getByRole('dialog', { name: 'Quick connect' })).toBeInTheDocument()
  expect(screen.getByText('Nothing is saved.')).toBeInTheDocument()
  await user.type(field('Host'), ' example.com ')
  await user.click(connect())
  expect(onconnect).toHaveBeenCalledExactlyOnceWith('example.com')
})

test('user and port are put around the host the way the core reads them', async () => {
  const { user, field, onconnect } = open()
  await user.type(field('Host'), 'example.com')
  await user.type(field('User'), 'root')
  await user.type(field('Port'), '2200')
  await user.click(connect())
  expect(onconnect).toHaveBeenLastCalledWith('root@example.com:2200')

  // A bare IPv6 address needs its brackets before a port can follow.
  await user.clear(field('Host'))
  await user.type(field('Host'), '::1')
  await user.click(connect())
  expect(onconnect).toHaveBeenLastCalledWith('root@[::1]:2200')
})

test('without a host nothing is sent', async () => {
  const { user, field, onconnect } = open()
  await user.type(field('User'), 'root')
  await user.click(connect())
  expect(onconnect).not.toHaveBeenCalled()
  expect(field('Host')).toBeRequired()
})

test('a refusal is shown in the form, which stays open', async () => {
  const refusal = { class: 'other', label: 'ERROR', message: 'expected user@host or user@host:port', chain: '' }
  const { user, field, onclose } = open(vi.fn().mockRejectedValue(refusal))
  await user.type(field('Host'), 'not a host')
  await user.click(connect())
  expect(await screen.findByRole('alert')).toHaveTextContent('ERROR expected user@host or user@host:port')
  expect(connect()).toBeEnabled()
  expect(onclose).not.toHaveBeenCalled()
})

test('cancel closes it', async () => {
  const { user, onclose, onconnect } = open()
  await user.click(screen.getByRole('button', { name: 'Cancel' }))
  expect(onclose).toHaveBeenCalledOnce()
  expect(onconnect).not.toHaveBeenCalled()
})
