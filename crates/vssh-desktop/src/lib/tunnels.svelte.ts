// The forwards that listen now, of every host. The shell says when one
// starts and when one ends by itself; what a person stops in the dialog ends
// without a word from the shell, so the dialog says what it sees.
import { untrack } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import type { Forward, ForwardState } from './api'

export const tunnels = new SvelteSet<string>()

const key = (host: string, row: Pick<Forward, 'kind' | 'value'>) => `${host} ${row.kind} ${row.value}`

export function heard(said: ForwardState) {
  if (said.state === 'on') tunnels.add(key(said.host, said))
  else tunnels.delete(key(said.host, said))
}

/** A host's forwards as they are now; none: the host is gone. */
export function sync(host: string, rows: Forward[]) {
  const on = rows.filter((row) => row.state === 'on').map((row) => key(host, row))
  // Called from an effect: what it reads of the set is not what it waits for.
  untrack(() => {
    for (const name of tunnels) if (name.startsWith(`${host} `) && !on.includes(name)) tunnels.delete(name)
    for (const name of on) tunnels.add(name)
  })
}
