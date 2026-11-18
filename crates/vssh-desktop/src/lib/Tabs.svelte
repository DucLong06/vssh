<script lang="ts">
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Keys from './Keys.svelte'
  import SessionDot from './SessionDot.svelte'
  import { capital, KEY, tip } from './keymap'

  export type Tab = {
    id: number
    title: string
    state: 'connecting' | 'up' | 'closed'
    /** The command its shell is running; `null` at the prompt. */
    busy: string | null
    /** What is typed into it goes to every broadcasting tab. */
    broadcast: boolean
  }
  type Props = {
    sessions: Tab[]
    /** The file views, which come after the sessions. */
    files?: { id: number; host: string }[]
    /** The session in front; `null` = the dashboard. */
    active: number | null
    /** The tabs that are open together, as panes. */
    split?: number[]
    /** Focus mode: the rail is away, and the way back to it is here. */
    zen?: boolean
    /** Ctrl-A waits for its key, where no status bar says so. */
    leader?: boolean
    onpick: (id: number | null) => void
    onclose: (id: number) => void
    /** A new tab: the quick connect. */
    onnew?: () => void
    /** Another shell on the host in front, beside it or (`down`) under it. */
    onsplit?: (down: boolean) => void
    /** The pane in front, back to a tab of its own. */
    onunsplit?: () => void
    onzen?: () => void
    /** A session's own name, typed over its tab; empty: its host's again. */
    onrename?: (id: number, name: string) => void
    /** A tab is picked up, or (`null`) let go. */
    ondrag?: (id: number | null) => void
  }
  let { sessions, files = [], active, split = [], zen = false, leader = false, onpick, onclose, onnew, onsplit, onunsplit, onzen, onrename, ondrag }: Props =
    $props()

  let list = $state<HTMLElement>()
  /** The session whose name is being typed. */
  let renaming = $state<number | null>(null)

  // Among many tabs the one in front may be past the strip's edge.
  $effect(() => {
    void active
    list?.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  })

  // A pill, filled with the terminal's ground when it is in front. The panes of a split wear one line;
  // broadcast takes the colours, so the tab in front is also bold, and only it has the full border.
  const pill =
    'group flex h-8 shrink-0 items-center rounded-[9px] border border-transparent text-[13px] text-dim hover:bg-bg/60 hover:text-fg has-[[aria-selected=true]]:bg-bg has-[[aria-selected=true]]:font-semibold has-[[aria-selected=true]]:text-ink data-[pane]:border-blue/35 data-[broadcast]:border-yellow/40! data-[broadcast]:text-yellow! data-[broadcast]:has-[[aria-selected=true]]:border-yellow!'
  const tab = 'flex h-full cursor-pointer items-center gap-2 rounded-[9px] pl-3'
  // There for the pointer that is on the tab, for the keyboard, and on the tab in front.
  const cross =
    'mx-1 grid size-5 cursor-pointer place-items-center rounded-md text-dim opacity-0 group-hover:opacity-100 group-has-[[aria-selected=true]]:opacity-100 hover:bg-selection hover:text-fg focus-visible:opacity-100'
  const digit = 'font-mono text-[11px] font-normal text-dim'
  const tool = 'grid size-8 shrink-0 cursor-pointer place-items-center rounded-lg text-dim hover:bg-selection hover:text-fg'
  /** A tab's number is its key after the leader. */
  const chord = (title: string, n: number) => (n < 10 ? tip(title, [`${KEY.leader[0]} ${n}`]) : title)
  /** Ctrl-W closes the tab in front, and no other. */
  const shut = (id: number) => (id === active ? tip(capital(t('close')), KEY.closeTab) : undefined)

  /**
   * What makes a tab one to pick up, and to close with the wheel's button. On
   * the pill, not on the button in it: WebKitGTK starts no drag from a button.
   * The keys for the same are the leader's and the strip's own buttons.
   */
  const held = (id: number) => ({
    // Not while its name is typed: there the pointer selects letters.
    draggable: renaming !== id,
    ondragstart: (e: DragEvent) => {
      // Some webviews do not start a drag that carries nothing.
      e.dataTransfer?.setData('text/plain', String(id))
      ondrag?.(id)
    },
    ondragend: () => ondrag?.(null),
    onauxclick: (e: MouseEvent) => {
      if (e.button === 1) onclose(id)
    },
  })

  /** The field over a tab takes the keys, with the name there to type over. */
  const taken = (field: HTMLInputElement) => {
    field.focus()
    field.select()
  }
</script>

<div class="flex shrink-0 items-center gap-1 border-b border-edge bg-panel px-2.5 py-2">
  {#if zen && onzen}
    <button
      type="button"
      onclick={onzen}
      aria-label={t('show_sidebar')}
      aria-keyshortcuts="Control+Shift+B Control+B"
      title={tip(t('show_sidebar'), KEY.focus)}
      class={tool}
    >
      <Icon name="rail" />
    </button>
  {/if}
  <div bind:this={list} role="tablist" aria-label={t('sessions')} class="flex min-w-0 items-center gap-1 overflow-x-auto">
    <span class={pill}>
      <button type="button" role="tab" aria-selected={active === null} title={chord(t('hosts'), 0)} onclick={() => onpick(null)} class="{tab} pr-3">
        <span class={digit}>0</span>
        {t('hosts')}
      </button>
    </span>
    {#each sessions as session, at (session.id)}
      <span class={pill} data-broadcast={session.broadcast || undefined} data-pane={split.includes(session.id) || undefined} {...held(session.id)}>
        {#if renaming === session.id}
          <input
            use:taken
            aria-label={t('rename_tab')}
            value={session.title}
            autocomplete="off"
            spellcheck="false"
            onkeydown={(e) => {
              if (e.key === 'Escape') renaming = null
              else if (e.key === 'Enter') e.currentTarget.blur()
            }}
            onblur={(e) => {
              // Esc let go of it already: what was typed is not the name.
              if (renaming === session.id) onrename?.(session.id, e.currentTarget.value.trim())
              renaming = null
            }}
            class="mx-1 h-6 w-32 rounded-md border border-blue/70 bg-page px-2 font-normal text-fg outline-none"
          />
        {:else}
          <button
            type="button"
            role="tab"
            aria-selected={session.id === active}
            title={chord(session.title, at + 1)}
            onclick={() => onpick(session.id)}
            ondblclick={() => onrename && (renaming = session.id)}
            class="{tab} pr-0.5"
          >
            <SessionDot {session} />
            <span class={digit}>{at + 1}</span>
            {session.title}
            {#if session.busy}
              <!-- Closing is at once, as in the TUI: this is the warning there is. -->
              <span class="rounded-full bg-bar px-2 text-xs font-normal text-white">{t('running')} <span class="font-mono">{session.busy}</span></span>
            {/if}
          </button>
        {/if}
        <button type="button" aria-label="{t('close')} {session.title}" title={shut(session.id)} onclick={() => onclose(session.id)} class={cross}>
          ×
        </button>
      </span>
    {/each}
    {#each files as view, at (view.id)}
      <span class={pill} data-pane={split.includes(view.id) || undefined} {...held(view.id)}>
        <button
          type="button"
          role="tab"
          aria-selected={view.id === active}
          title={chord(`${t('files')} ${view.host}`, sessions.length + at + 1)}
          onclick={() => onpick(view.id)}
          class="{tab} pr-0.5"
        >
          <Icon name="folder" class="size-3.5! text-cyan" />
          <span class={digit}>{sessions.length + at + 1}</span>
          <!-- Said, and drawn as the folder before it. -->
          <span class="sr-only">{t('files')}</span>
          {view.host}
        </button>
        <button type="button" aria-label="{t('close')} {t('files')} {view.host}" title={shut(view.id)} onclick={() => onclose(view.id)} class={cross}>
          ×
        </button>
      </span>
    {/each}
  </div>
  {#if onnew}
    <button type="button" onclick={onnew} aria-label={t('new_tab')} aria-keyshortcuts="Control+T" title={tip(t('new_tab'), KEY.newTab)} class={tool}>
      <Icon name="plus" />
    </button>
  {/if}
  <div class="ml-auto flex shrink-0 items-center gap-0.5 pl-2">
    {#if leader}
      <!-- Half a chord, as the status bar says it where there is one. -->
      <span role="status" class="pr-1"><Keys keys={KEY.leader[0]} class="text-yellow" /></span>
    {/if}
    {#if active !== null}
      {#if onunsplit && split.includes(active)}
        <button type="button" onclick={onunsplit} aria-label={t('unsplit')} title={tip(t('unsplit'), [`${KEY.leader[0]} !`])} class={tool}>
          <Icon name="whole" />
        </button>
      {/if}
      {#if onsplit}
        <button
          type="button"
          onclick={() => onsplit(false)}
          aria-label={t('split_right')}
          aria-keyshortcuts="Control+Shift+D"
          title={tip(t('split_right'), KEY.splitRight)}
          class={tool}
        >
          <Icon name="right" />
        </button>
        <button
          type="button"
          onclick={() => onsplit(true)}
          aria-label={t('split_down')}
          aria-keyshortcuts="Control+Shift+E"
          title={tip(t('split_down'), KEY.splitDown)}
          class={tool}
        >
          <Icon name="below" />
        </button>
      {/if}
    {/if}
  </div>
</div>
