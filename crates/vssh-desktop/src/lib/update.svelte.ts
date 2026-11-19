// Whether a newer version is out: one question to GitHub's public API, some
// time after the app has started. Nothing of the user or of this machine
// goes with it, and nothing is downloaded by it. No answer, whatever the
// reason, is no news. The newer version is fetched and installed (D15) only
// when a person asks for it: `install`, by the line's button.
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'
import { api, type ConnError } from './api'

const LATEST = 'https://api.github.com/repos/DucLong06/vssh/releases/latest'

type Update = {
  /** The tag of a newer release; nothing once it was dismissed, for as long as the app runs. */
  tag: string | null
  /** How many bytes its `.deb` has, if the release has one and this app came as one: there is a button for it then. */
  deb: number | null
  /** Where an update that was asked for is; `manual`: it is fetched, and is the user's to install. */
  phase: 'downloading' | 'installing' | 'installed' | 'manual' | 'download-failed' | 'install-failed' | null
  /** Bytes of the `.deb` that are here. */
  done: number
  /** Why it failed, in the tools' own words. */
  why: string
  /** The `.deb`, to install by hand. */
  file: string
  /** The dialog that says how it ended is open. */
  dialog: boolean
}
const none: Update = { tag: null, deb: null, phase: null, done: 0, why: '', file: '', dialog: false }

export const update = $state<Update>({ ...none })

/** Says no more of it: dismissed. */
export const forget = () => void Object.assign(update, none)

/** Whether version `a` is past `b`, by their numbers; with a `v` in front or without. */
export function newer(a: string, b: string): boolean {
  const numbers = (version: string) => version.replace(/^v/, '').split('.').map((part) => parseInt(part, 10) || 0)
  const [x, y] = [numbers(a), numbers(b)]
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0)
  }
  return false
}

/** Asks once. Never rejects, and says nothing of a failure. */
export async function check(): Promise<void> {
  try {
    // No cookies, no referrer, nothing kept: a public GET and no more.
    const answer = await fetch(LATEST, {
      credentials: 'omit',
      referrerPolicy: 'no-referrer',
      cache: 'no-store',
      signal: AbortSignal.timeout(10_000),
    })
    if (!answer.ok) return
    const { tag_name: tag, assets } = await answer.json()
    // It is put on screen: only what looks like a version.
    if (typeof tag !== 'string' || !/^v?\d+(\.\d+)*$/.test(tag) || !newer(tag, await getVersion())) return
    const name = `vssh-desktop_${tag.replace(/^v/, '')}_amd64.deb`
    const deb: unknown = Array.isArray(assets) ? assets.find((asset) => asset?.name === name)?.size : undefined
    // The button is for a release that has the .deb, in an app that came as one.
    update.deb = typeof deb === 'number' && (await api.updatePossible().catch(() => false)) ? deb : null
    update.tag = tag
  } catch {
    // No network, a dead proxy, an answer that is no JSON.
  }
}

/**
 * Fetches the newer version's `.deb` and installs it: on a person's click
 * and never otherwise. A click again starts over, from the download. Never
 * rejects: how it ended is in `update`.
 */
export async function install(): Promise<void> {
  const tag = update.tag
  if (!tag || update.phase === 'downloading' || update.phase === 'installing') return
  update.done = 0
  update.phase = 'downloading'
  const heard = listen<number>('update-progress', ({ payload }) => (update.done = payload))
  try {
    await api.updateDownload(tag)
    update.phase = 'installing'
    const file = await api.updateInstall(tag)
    update.file = file ?? ''
    update.phase = file === null ? 'installed' : 'manual'
    update.dialog = true
  } catch (error) {
    update.why = (error as ConnError).chain?.join(': ') ?? String(error)
    update.phase = update.phase === 'downloading' ? 'download-failed' : 'install-failed'
  } finally {
    void heard.then((unlisten) => unlisten())
  }
}
