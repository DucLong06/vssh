<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { api, explain, type Hop, type Host, type HostForm, type Shown, type VaultState } from './api'
  import HostImport from './HostImport.svelte'
  import { i18n, t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { streamer, target } from './streamer.svelte'

  type Props = {
    /** The hosts there are: an alias is taken once, and one of them may be the jump host. */
    hosts: Host[]
    /** The host to change; none: a new one. */
    alias?: string
    /** What a new host's form opens with: a quick connect. */
    start?: Partial<HostForm>
    vault: VaultState
    onsaved: (host: Host) => void
    /** Asked for, not done: the window asks first. */
    ondelete: (alias: string) => void
    /** The key setup, for a host that is signed in to with a password. */
    onkeys: (alias: string) => void
    onimported: (aliases: string[]) => void
    onclose: () => void
  }
  let { hosts, alias, start, vault, onsaved, ondelete, onkeys, onimported, onclose }: Props = $props()

  type Method = 'password' | 'key' | 'agent'
  const opening = (): HostForm => ({
    alias: '',
    hostname: '',
    user: '',
    port: null,
    identity_file: '',
    proxy_jump: '',
    grp: '',
    tags: '',
    notes: '',
    pinned: false,
    ...start,
  })
  let form = $state(opening())
  /** As it is typed: what is no number must be there to be called wrong. */
  let port = $state('')
  let tags = $state<string[]>([])
  /** The tag being typed. */
  let tag = $state('')
  let method = $state<Method>('agent')
  /** For the vault. What is stored there is never shown. */
  let password = $state('')
  /** The vault has a password for this host. */
  let had = $state(false)
  /** What is wrong, by field, since the form was last sent; a field that is typed in is not called wrong meanwhile. */
  let wrong = $state<{ alias?: string; hostname?: string; port?: string; hops?: string }>({})
  /** The road to the host: its jump hosts in order, each as `ProxyJump` writes it, with the key it is given here, if any. */
  let road = $state<Hop[]>([])
  /** The forms of the hosts on the road: what each is logged in to with when nothing is said here. */
  let known = $state<Record<string, HostForm>>({})
  const sought = new Set<string>()
  let failed = $state<Shown | null>(null)
  let importing = $state(false)
  let root = $state<HTMLFormElement>()

  /** Shows the form's host in the fields that are not the form's own. */
  function show() {
    port = String(form.port ?? '')
    tags = form.tags.split(/[ ,]+/).filter(Boolean)
    method = form.identity_file ? 'key' : had ? 'password' : 'agent'
    const given = form.jump_auth ?? []
    const hops = form.proxy_jump.split(',').map((hop) => hop.trim())
    road = hops.filter(Boolean).map((hop) => ({ hop, key: '', user: '', ...given.find((own) => own.hop === hop) }))
  }
  show()

  onMount(async () => {
    root?.querySelector('input')?.focus()
    if (!alias) return
    try {
      // A locked vault says nothing of what it holds.
      had = vault === 'unlocked' && (await api.vaultHosts()).includes(alias)
      form = await api.hostsForm(alias)
      show()
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  })

  function addTag() {
    const name = tag.trim()
    if (name && !tags.includes(name)) tags.push(name)
    tag = ''
  }

  function tagKey(e: KeyboardEvent) {
    // Enter in the empty field is the form's Enter.
    const ends = e.key === ' ' || e.key === ',' || (e.key === 'Enter' && tag.trim())
    if (ends) {
      e.preventDefault()
      addTag()
    } else if (e.key === 'Backspace' && !tag) {
      tags.pop()
    }
  }

  /** The host a hop names: without its `user@` and its `:port`. */
  const host = (hop: string) => hop.trim().replace(/^.*@/, '').replace(/:\d+$/, '')
  const mine = (name: string) => hosts.some((other) => other.alias === name)
  /** A hop as the window may show it: a host's alias, or an address, which streamer mode makes up. */
  const named = (hop: string) => (mine(host(hop)) ? hop.trim() : target(hop.trim()))
  /** An alias or `user@host[:port]`, and not the host itself: that would be a loop. */
  const passable = (hop: string) => /^([^\s,@]+@)?[^\s,@:]+(:\d+)?$/.test(hop.trim()) && host(hop) !== form.alias.trim()

  // A jump host's own block says what it is logged in to with: read once for each host on the road.
  $effect(() => {
    for (const { hop } of road) {
      const name = host(hop)
      if (!mine(name) || sought.has(name)) continue
      sought.add(name)
      api.hostsForm(name).then(
        (its) => (known[name] = its),
        () => {},
      )
    }
  })

  /** What a hop is logged in to with: the key chosen here before the jump host's own, and the agent where there is neither. */
  // No password: the core logs in to a jump host with keys only.
  function via(hop: Hop) {
    const its = known[host(hop.hop)]?.identity_file
    const how = hop.key.trim() ? `${t('hop_own_key')} ${hop.key.trim()}` : its ? `${t('hop_its_key')} ${its}` : t('hop_agent')
    return t('hop_via').replace('{name}', named(hop.hop)).replace('{method}', how)
  }

  /** One more hop at the end of the road; an empty one is typed into at once. */
  async function addHop(hop = '') {
    road.push({ hop, key: '', user: '' })
    wrong.hops = undefined
    await tick()
    if (!hop) root?.querySelector<HTMLElement>('[data-road] li:last-child input')?.focus()
  }

  /** Asks for a key file in the desktop's dialog; closed without one, what was there stays. */
  async function choose(put: (path: string) => void) {
    try {
      const path = await api.pickFile(t('choose_key'), [t('key_files'), t('all_files')])
      if (path) put(path)
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  }

  /** The rules of the shell's form, and the window's own: a host, and a port that is one. */
  function check() {
    const name = form.alias.trim()
    const taken = name !== alias && hosts.some((host) => host.alias === name)
    const number = /^\d+$/.test(port.trim()) && Number(port) >= 1 && Number(port) <= 65535
    wrong = {
      alias:
        !name || /[\s*?!#"=]/.test(name) ? t('alias_invalid') : taken ? t('exists').replace('{name}', name) : undefined,
      hostname: form.hostname.trim() ? undefined : t('host_required'),
      port: !port.trim() || number ? undefined : t('port_invalid'),
      // A row left empty is no hop.
      hops: road.every((hop) => !hop.hop.trim() || passable(hop.hop)) ? undefined : t('hop_invalid'),
    }
    return !Object.values(wrong).some(Boolean)
  }

  async function save(e: SubmitEvent) {
    e.preventDefault()
    failed = null
    addTag()
    if (!check()) {
      await tick()
      root?.querySelector<HTMLElement>('[aria-invalid]')?.focus()
      return
    }
    const hops = road.map((hop) => ({ hop: hop.hop.trim(), key: hop.key.trim(), user: hop.user.trim() })).filter((hop) => hop.hop)
    const sent: HostForm = {
      ...form,
      alias: form.alias.trim(),
      hostname: form.hostname.trim(),
      user: form.user.trim(),
      port: port.trim() ? Number(port) : null,
      identity_file: method === 'key' ? form.identity_file.trim() : '',
      // As ssh writes the road; the keys are the app's own to keep, and a user goes with a key.
      proxy_jump: hops.map((hop) => hop.hop).join(','),
      jump_auth: hops.filter((hop) => hop.key),
      tags: tags.join(' '),
    }
    // A host that has a key now, or the agent, is not sent its old password any more.
    if (vault === 'unlocked' && method === 'password' && password) sent.password = password
    else if (vault === 'unlocked' && method !== 'password' && had) sent.password = ''
    try {
      onsaved(alias ? await api.hostsUpdate(alias, sent) : await api.hostsAdd(sent))
    } catch (error) {
      failed = await explain(error, i18n.lang)
    }
  }

  const title = $derived(alias ? t('edit_host') : t('add_host'))
  const methods = $derived<[Method, string][]>([
    ['password', t('auth_password')],
    ['key', t('auth_key')],
    ['agent', t('auth_agent')],
  ])
  /** Every other host that is not on the road yet. */
  const offered = $derived(
    hosts.map((other) => other.alias).filter((name) => name !== alias && !road.some((hop) => hop.hop.trim() === name)),
  )
  const nth = (at: number) => t('hop').replace('{n}', String(at + 1))

  const field =
    'mt-1 w-full rounded-lg border border-edge bg-bg px-3 py-2 text-fg placeholder:text-dim disabled:opacity-60 aria-invalid:border-red'
  const button = 'cursor-pointer rounded-[10px] border border-edge px-3.5 py-1.5 font-medium whitespace-nowrap hover:bg-selection'
  const choice =
    'cursor-pointer rounded-md px-2.5 py-1 text-dim hover:text-fg has-checked:bg-selection has-checked:font-medium has-checked:text-ink has-focus-visible:outline-2 has-focus-visible:outline-blue'
  const plain = { autocomplete: 'off', autocapitalize: 'off', spellcheck: false } as const
  const small = 'w-full rounded-lg border border-edge bg-bg px-2.5 py-1 font-mono text-xs text-fg placeholder:font-sans placeholder:text-dim disabled:opacity-40 aria-invalid:border-red'
  const tool = 'grid shrink-0 cursor-pointer place-items-center rounded-lg border border-edge text-dim hover:bg-selection hover:text-fg disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent'
  const offer = 'cursor-pointer rounded-full border border-edge px-2 py-0.5 hover:border-blue hover:text-blue'
</script>

<!-- After the field's label, not in it: it is no part of the field's name. -->
{#snippet problem(id: string, words: string | undefined)}
  {#if words}
    <p {id} role="alert" class="text-red">{words}</p>
  {/if}
{/snippet}

<!-- Wide, with four fields to a row: all of it is in view in a window 700 px high. -->
<Modal label={importing ? t('import_ssh_config') : title} wide {onclose}>
  {#if importing}
    <HostImport {onimported} oncancel={() => (importing = false)} />
  {:else}
    <form bind:this={root} novalidate onsubmit={save} class="grid max-h-[84vh] gap-4 overflow-y-auto p-6 text-[13px] text-white">
      <p class="text-base font-semibold text-ink">{title}</p>
      <div class="grid grid-cols-[1fr_1.5fr_1fr_6rem] gap-2">
        <div>
          <label>
            {t('alias')}
            <input
              bind:value={form.alias}
              oninput={() => (wrong.alias = undefined)}
              {...plain}
              aria-invalid={wrong.alias ? 'true' : undefined}
              aria-describedby="host-form-alias"
              class={field}
            />
          </label>
          {@render problem('host-form-alias', wrong.alias)}
        </div>
        <div>
          <label>
            {t('host')}
            <input
              bind:value={form.hostname}
              type={streamer.on ? 'password' : 'text'}
              oninput={() => (wrong.hostname = undefined)}
              {...plain}
              aria-invalid={wrong.hostname ? 'true' : undefined}
              aria-describedby="host-form-hostname"
              class="{field} font-mono text-[13px]"
            />
          </label>
          {@render problem('host-form-hostname', wrong.hostname)}
        </div>
        <label>
          {t('user')}
          <input bind:value={form.user} {...plain} class={field} />
        </label>
        <div>
          <label>
            {t('port')}
            <input
              bind:value={port}
              oninput={() => (wrong.port = undefined)}
              inputmode="numeric"
              placeholder="22"
              {...plain}
              aria-invalid={wrong.port ? 'true' : undefined}
              aria-describedby="host-form-port"
              class="{field} font-mono text-[13px]"
            />
          </label>
          {@render problem('host-form-port', wrong.port)}
        </div>
      </div>

      <div class="grid grid-cols-[auto_1fr] items-end gap-2">
        <div>
          <span id="host-form-method">{t('sign_in_with')}</span>
          <!-- Radio buttons, drawn as one control: as the dashboard's sort. Named: nameless, the webview takes
               this group and the jump hosts for one, and Tab stops at one of the two only. -->
          <div
            role="radiogroup"
            aria-labelledby="host-form-method"
            class="mt-1 flex rounded-lg border border-edge bg-page p-1"
          >
            {#each methods as [value, label] (value)}
              <label class={choice}>
                <input type="radio" name="host-form-method" bind:group={method} {value} class="sr-only" />
                {label}
              </label>
            {/each}
          </div>
        </div>
        {#if method === 'password'}
          <label>
            {t('password')}
            <input
              type="password"
              bind:value={password}
              autocomplete="off"
              disabled={vault !== 'unlocked'}
              placeholder={had ? t('password_stored') : ''}
              aria-describedby="host-form-vault"
              class={field}
            />
          </label>
          {#if vault !== 'unlocked'}
            <p id="host-form-vault" class="col-span-2 text-yellow">{t('vault_needed')}</p>
          {/if}
        {:else if method === 'key'}
          <div class="flex items-end gap-1.5">
            <label class="min-w-0 flex-1">
              {t('key_file')}
              <input bind:value={form.identity_file} placeholder="~/.ssh/id_ed25519" {...plain} class="{field} font-mono text-[13px]" />
            </label>
            <button
              type="button"
              aria-label={t('choose_key')}
              title={t('choose_key')}
              onclick={() => choose((path) => (form.identity_file = path))}
              class="{tool} size-[38px]"
            >
              <Icon name="folder" />
            </button>
          </div>
        {/if}
      </div>

      <div role="group" aria-labelledby="host-form-jump" aria-describedby="host-form-hops" class="grid gap-1.5">
        <p><span id="host-form-jump">{t('jump_host')}</span> <span class="text-dim">· {t('jump_note')}</span></p>
        {#if road.length}
          <!-- The road as a picture: this machine, each hop, the host. Resting on a hop says what it is logged in to with. -->
          <ol aria-label={t('jump_path')} class="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border border-edge bg-page px-3 py-1.5 text-xs">
            <li class="flex items-center gap-1.5"><Icon name="term" class="size-4" />{t('this_machine')}</li>
            {#each road.filter((hop) => hop.hop.trim()) as hop, at (at)}
              <li title={via(hop)} class="flex items-center gap-1.5 text-fg">
                <span aria-hidden="true" class="text-dim">→</span>
                <Icon name="server" class="size-4 {hop.key.trim() ? 'text-yellow' : 'text-cyan'}" />
                {named(hop.hop)}
              </li>
            {/each}
            <li class="flex items-center gap-1.5 font-medium text-ink">
              <span aria-hidden="true" class="text-dim">→</span>
              <Icon name="server" class="size-4 text-blue" />
              {form.alias.trim() || t('target')}
            </li>
          </ol>
          <!-- A hop to a row: who, a key of its own, a user to go with it; earlier, later, off the road. -->
          <ol data-road aria-label={t('jump_host')} class="grid gap-1.5">
            {#each road as hop, at (at)}
              <li class="grid grid-cols-[1.1fr_1.5fr_auto_0.9fr_auto] items-center gap-1.5">
                <input
                  bind:value={hop.hop}
                  type={streamer.on && !mine(host(hop.hop)) ? 'password' : 'text'}
                  oninput={() => (wrong.hops = undefined)}
                  aria-label={nth(at)}
                  aria-invalid={wrong.hops && hop.hop.trim() && !passable(hop.hop) ? 'true' : undefined}
                  placeholder={t('hop_hint')}
                  {...plain}
                  class={small}
                />
                <input
                  bind:value={hop.key}
                  aria-label={t('hop_key_of').replace('{n}', String(at + 1))}
                  title={t('hop_key')}
                  placeholder={t('hop_key')}
                  {...plain}
                  class={small}
                />
                <button
                  type="button"
                  aria-label="{t('choose_key')} · {nth(at)}"
                  title={t('choose_key')}
                  onclick={() => choose((path) => (hop.key = path))}
                  class="{tool} size-[26px]"
                >
                  <Icon name="folder" class="size-4" />
                </button>
                <!-- A user goes with a key: without one the jump host's own block is all there is. -->
                <input
                  bind:value={hop.user}
                  disabled={!hop.key.trim()}
                  aria-label={t('hop_user_of').replace('{n}', String(at + 1))}
                  placeholder={t('hop_user')}
                  {...plain}
                  class={small}
                />
                <span class="flex gap-1">
                  <button
                    type="button"
                    aria-label="{t('move_up')} · {nth(at)}"
                    title={t('move_up')}
                    disabled={at === 0}
                    onclick={() => road.splice(at - 1, 2, road[at], road[at - 1])}
                    class="{tool} size-[26px]">↑</button
                  >
                  <button
                    type="button"
                    aria-label="{t('move_down')} · {nth(at)}"
                    title={t('move_down')}
                    disabled={at === road.length - 1}
                    onclick={() => road.splice(at, 2, road[at + 1], road[at])}
                    class="{tool} size-[26px]">↓</button
                  >
                  <button
                    type="button"
                    aria-label="{t('delete')} · {nth(at)}"
                    title={t('delete')}
                    onclick={() => road.splice(at, 1)}
                    class="{tool} size-[26px] hover:text-red">×</button
                  >
                </span>
                {#if hop.hop.trim()}
                  <p class="col-span-full -mt-0.5 truncate pl-1 text-xs text-dim">{via(hop)}</p>
                {/if}
              </li>
            {/each}
          </ol>
        {/if}
        <!-- One stop for the keys, which type the hop into its row; the hosts there are, for the pointer. -->
        <div class="flex max-h-14 flex-wrap items-center gap-1 overflow-y-auto">
          <button type="button" onclick={() => addHop()} class="{offer} border-dashed text-fg">+ {t('add_hop')}</button>
          {#each offered as name (name)}
            <button type="button" tabindex="-1" aria-label="{t('add_hop')}: {name}" onclick={() => addHop(name)} class={offer}>{name}</button>
          {/each}
        </div>
        {@render problem('host-form-hops', wrong.hops)}
      </div>

      <div class="grid grid-cols-[1fr_12rem_auto] items-end gap-3">
        <div>
          <span>{t('tags')}</span>
          <div class="mt-1 flex flex-wrap items-center gap-1 rounded-lg border border-edge bg-bg px-2 py-1">
            <ul aria-label={t('tags')} class="contents">
              {#each tags as name (name)}
                <li
                  class="flex items-center gap-1 rounded-md bg-bar px-1.5 text-xs leading-5 text-white"
                >
                  <span>{name}</span>
                  <button
                    type="button"
                    aria-label="{t('delete')} {name}"
                    onclick={() => (tags = tags.filter((other) => other !== name))}
                    class="cursor-pointer hover:text-red"
                  >
                    ×
                  </button>
                </li>
              {/each}
            </ul>
            <input
              bind:value={tag}
              onkeydown={tagKey}
              aria-label={t('add_tag')}
              placeholder={t('add_tag')}
              {...plain}
              class="min-w-24 flex-1 bg-transparent py-0.5 text-fg outline-none placeholder:text-dim"
            />
          </div>
        </div>
        <label>
          {t('group')}
          <input bind:value={form.grp} {...plain} class={field} />
        </label>
        <label class="flex cursor-pointer items-center gap-2 py-1.5">
          <input type="checkbox" bind:checked={form.pinned} class="accent-yellow" />
          {t('pinned')}
        </label>
      </div>
      <label>
        {t('notes')}
        <textarea bind:value={form.notes} rows="2" class="{field} resize-none"></textarea>
      </label>

      {#if failed}
        <p role="alert" title={failed.chain} class="break-words">
          <strong class="text-red">✗ {failed.label}</strong>
          <span>{failed.message}</span>
        </p>
      {/if}
      <div class="flex flex-wrap gap-2">
        {#if alias}
          <button type="button" onclick={() => ondelete(alias)} class="{button} border-red text-red">
            {t('delete')}
          </button>
          {#if had && method === 'password'}
            <button type="button" onclick={() => onkeys(alias)} class="{button} text-cyan">{t('key_setup')}</button>
          {/if}
        {:else}
          <button type="button" onclick={() => (importing = true)} class="{button} text-cyan">
            {t('import_ssh_config')}
          </button>
        {/if}
        <button type="button" onclick={onclose} class="{button} ml-auto">{t('cancel')}</button>
        <button type="submit" class="{button} border-blue bg-blue! font-semibold text-page hover:brightness-110">{t('save')}</button>
      </div>
    </form>
  {/if}
</Modal>
