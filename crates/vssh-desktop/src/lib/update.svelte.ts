// Whether a newer version is out: one question to GitHub's public API, some
// time after the app has started. Nothing of the user or of this machine
// goes with it, and nothing is downloaded. No answer, whatever the reason,
// is no news.
import { getVersion } from '@tauri-apps/api/app'

const LATEST = 'https://api.github.com/repos/DucLong06/vssh/releases/latest'

/** The tag of a newer release; nothing once it was dismissed, for as long as the app runs. */
export const update = $state<{ tag: string | null }>({ tag: null })

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
    const { tag_name: tag } = await answer.json()
    // It is put on screen: only what looks like a version.
    if (typeof tag === 'string' && /^v?\d+(\.\d+)*$/.test(tag) && newer(tag, await getVersion())) update.tag = tag
  } catch {
    // No network, a dead proxy, an answer that is no JSON.
  }
}
