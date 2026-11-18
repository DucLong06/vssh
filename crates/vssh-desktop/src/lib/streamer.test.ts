import { render, screen } from '@testing-library/svelte'
import { expect, test, vi } from 'vitest'
import type { Host } from './api'
import fixture from './fixtures/hosts.json'
import HostCard from './HostCard.svelte'
import { addr, fake, hide, learn, scrub, setStreamer, streamer, target } from './streamer.svelte'

const hosts: Host[] = fixture
const [web01, web02, db] = hosts
/** The two documentation ranges: addresses that are nobody's. */
const MADE_UP = /^(203\.0\.113|198\.51\.100)\.([1-9]\d?|1\d\d|2[0-4]\d|25[0-4])$/

test('three hosts get three made-up addresses, each from a range that is nobody\'s, the same every time', () => {
  learn(hosts)
  const ips = [web01, web02, db].map((host) => fake(host.alias))
  expect(new Set(ips).size).toBe(3)
  for (const ip of ips) expect(ip).toMatch(MADE_UP)
  // Asked again, and after the hosts were read again: the same ones.
  expect([web01, web02, db].map((host) => fake(host.alias))).toEqual(ips)
  learn([...hosts])
  expect([web01, web02, db].map((host) => fake(host.alias))).toEqual(ips)
  // A name the config does not have still gets one, the same every time.
  expect(fake('not-a-host')).toMatch(MADE_UP)
  expect(fake('not-a-host')).toBe(fake('not-a-host'))
})

test('more hosts than two names can hash apart: every one still has an address of its own', () => {
  const many = Array.from({ length: 400 }, (_, i): Host => ({ ...web01, alias: `host-${i}` }))
  learn(many)
  expect(new Set(many.map((host) => fake(host.alias))).size).toBe(400)
})

test('on, a host is shown at its made-up address with its own user and port; off, at the real one', () => {
  learn(hosts)
  expect(addr(web01)).toBe('deploy@10.0.0.5:2222')
  setStreamer(true)
  expect(addr(web01)).toBe(`deploy@${fake('web-01')}:2222`)
  expect(addr(db)).toBe(`postgres@${fake('db-primary')}`)
  // A host that is nothing but a name has no address to hide.
  expect(addr(hosts[3])).toBe('')
  setStreamer(false)
  expect(addr(web01)).toBe('deploy@10.0.0.5:2222')
  // What is shown is all that changed.
  expect(hosts).toEqual(fixture)
})

test('a card follows the switch and keeps its address through a re-render', async () => {
  learn(hosts)
  setStreamer(true)
  const { rerender } = render(HostCard, { host: web01 })
  const card = screen.getByRole('article', { name: 'web-01' })
  const masked = `deploy@${fake('web-01')}:2222`
  // A card with nothing wrong says its address under its name.
  const said = () => card.querySelector('p.font-mono')
  expect(said()).toHaveTextContent(masked)
  await rerender({ host: { ...web01 }, marked: true })
  expect(said()).toHaveTextContent(masked)
  setStreamer(false)
  await vi.waitFor(() => expect(said()).toHaveTextContent('deploy@10.0.0.5:2222'))
})

test('the switch is kept as streamer_mode and read when the app starts', async () => {
  expect(streamer.on).toBe(false)
  setStreamer(true)
  expect(localStorage.getItem('vssh.streamer_mode')).toBe('true')
  vi.resetModules()
  expect((await import('./streamer.svelte')).streamer.on).toBe(true)
  setStreamer(false)
  expect(localStorage.getItem('vssh.streamer_mode')).toBe('false')
  vi.resetModules()
  expect((await import('./streamer.svelte')).streamer.on).toBe(false)
})

test('what a quick connect was typed as keeps its user and port and loses its host', () => {
  expect(target('root@192.0.2.9:2200')).toBe('root@192.0.2.9:2200')
  setStreamer(true)
  expect(target('root@192.0.2.9:2200')).toBe(`root@${fake('192.0.2.9')}:2200`)
  expect(target('example.org')).toBe(fake('example.org'))
  expect(target('example.org')).toMatch(MADE_UP)
  // This machine is nobody's secret.
  expect(target('me@localhost:2222')).toBe('me@localhost:2222')
  expect(hide('127.0.0.1')).toBe('127.0.0.1')
})

test('the words of a failure lose every address: a host\'s own becomes the one its card shows', () => {
  learn(hosts)
  const said = 'Connection refused by 10.0.0.5:2222 — is sshd running?'
  expect(scrub(said)).toBe(said)
  setStreamer(true)
  expect(scrub(said)).toBe(`Connection refused by ${fake('web-01')}:2222 — is sshd running?`)
  expect(scrub('Cannot resolve db.internal.example.com.')).toBe(`Cannot resolve ${fake('db-primary')}.`)
  // An address that is no host's is made up too; one that is made up already, or this machine's, stays.
  const hop = scrub('Jump host bastion failed: 192.0.2.1:22 refused')
  expect(hop).not.toContain('192.0.2.1')
  expect(hop).toMatch(/^Jump host bastion failed: (203\.0\.113|198\.51\.100)\.\d+:22 refused$/)
  expect(scrub(hop)).toBe(hop)
  expect(scrub('Port 127.0.0.1:8080 is already in use')).toBe('Port 127.0.0.1:8080 is already in use')
  // Only whole names and whole addresses: 10.0.0.5 is not in 10.0.0.55, nor a host named db in db-primary.
  learn([...hosts, { ...web01, alias: 'short', hostname: 'db' }])
  expect(scrub('10.0.0.55 and 110.0.0.5')).not.toContain(fake('web-01'))
  expect(scrub('db-primary: cannot resolve db: no such name')).toBe(`db-primary: cannot resolve ${fake('short')}: no such name`)
})
