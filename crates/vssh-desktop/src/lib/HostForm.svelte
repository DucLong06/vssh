<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { api, explain, type Host, type HostForm, type Shown, type VaultState } from './api'
  import HostImport from './HostImport.svelte'
  import { chip } from './hosts'
  import { i18n, t } from './i18n.svelte'
  import Modal from './Modal.svelte'

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
  let wrong = $state<{ alias?: string; hostname?: string; port?: string }>({})
  let failed = $state<Shown | null>(null)
  let importing = $state(false)
  let root = $state<HTMLFormElement>()

  /** Shows the form's host in the fields that are not the form's own. */
  function show() {
    port = String(form.port ?? '')
    tags = form.tags.split(/[ ,]+/).filter(Boolean)
    method = form.identity_file ? 'key' : had ? 'password' : 'agent'
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
    const sent: HostForm = {
      ...form,
      alias: form.alias.trim(),
      hostname: form.hostname.trim(),
      user: form.user.trim(),
      port: port.trim() ? Number(port) : null,
      identity_file: method === 'key' ? form.identity_file.trim() : '',
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
  /** Every other host, and whatever the block names now. */
  const jumps = $derived([
    ...new Set(['', ...hosts.map((host) => host.alias).filter((name) => name !== alias), form.proxy_jump]),
  ])

  const field =
    'mt-1 w-full rounded-md border border-edge bg-bg px-2 py-1.5 text-fg placeholder:text-dim disabled:opacity-60 aria-invalid:border-red'
  const button = 'cursor-pointer rounded-md border border-edge px-3 py-1.5 whitespace-nowrap hover:bg-selection'
  const choice =
    'cursor-pointer rounded px-2 py-1 text-white hover:text-fg has-checked:bg-selection has-checked:text-blue has-focus-visible:outline-2 has-focus-visible:outline-blue'
  const plain = { autocomplete: 'off', autocapitalize: 'off', spellcheck: false } as const
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
    <form bind:this={root} novalidate onsubmit={save} class="grid gap-3 p-4 text-white">
      <p class="text-cyan">{title}</p>
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
              oninput={() => (wrong.hostname = undefined)}
              {...plain}
              aria-invalid={wrong.hostname ? 'true' : undefined}
              aria-describedby="host-form-hostname"
              class={field}
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
              class={field}
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
            class="mt-1 flex rounded-md border border-edge bg-panel p-0.5"
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
          <label>
            {t('key_file')}
            <input bind:value={form.identity_file} placeholder="~/.ssh/id_ed25519" {...plain} class={field} />
          </label>
        {/if}
      </div>

      <div>
        <span id="host-form-jump">{t('jump_host')}</span>
        <!-- Chips, as the snippets' host: a native select opens the toolkit's own light menu. -->
        <div
          role="radiogroup"
          aria-labelledby="host-form-jump"
          class="mt-1 flex max-h-14 flex-wrap gap-1 overflow-y-auto"
        >
          {#each jumps as name (name)}
            <label
              class="cursor-pointer rounded-full border border-edge px-2 py-0.5 hover:text-fg has-checked:border-blue has-checked:bg-selection has-checked:text-blue has-focus-visible:outline-2 has-focus-visible:outline-blue"
            >
              <input type="radio" name="host-form-jump" bind:group={form.proxy_jump} value={name} class="sr-only" />
              {name || t('none')}
            </label>
          {/each}
        </div>
      </div>

      <div class="grid grid-cols-[1fr_12rem_auto] items-end gap-3">
        <div>
          <span>{t('tags')}</span>
          <div class="mt-1 flex flex-wrap items-center gap-1 rounded-md border border-edge bg-bg px-2 py-1">
            <ul aria-label={t('tags')} class="contents">
              {#each tags as name (name)}
                <li
                  class="flex items-center gap-1 rounded-full border px-2 text-xs leading-5"
                  style="color: {chip(name)}; border-color: color-mix(in srgb, {chip(name)} 40%, transparent)"
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
        <button type="submit" class="{button} border-blue bg-selection text-blue">{t('save')}</button>
      </div>
    </form>
  {/if}
</Modal>
