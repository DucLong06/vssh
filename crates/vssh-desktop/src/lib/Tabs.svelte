<script lang="ts">
  import { t } from './i18n.svelte'
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
    onpick: (id: number | null) => void
    onclose: (id: number) => void
  }
  let { sessions, files = [], active, onpick, onclose }: Props = $props()

  // The concept's tab: a pill, filled with the terminal's ground when it is in front.
  // Broadcast takes the colours, so the tab in front is also bold, and only it has the full border.
  const pill =
    'flex h-8 shrink-0 items-center rounded-[9px] border border-transparent text-[13px] text-dim hover:text-fg has-[[aria-selected=true]]:bg-bg has-[[aria-selected=true]]:font-semibold has-[[aria-selected=true]]:text-ink data-[broadcast]:border-yellow/40! data-[broadcast]:text-yellow! data-[broadcast]:has-[[aria-selected=true]]:border-yellow!'
  const tab = 'flex h-full cursor-pointer items-center gap-2 rounded-[9px] pl-3.5'
  const cross = 'mx-1 cursor-pointer rounded-md px-1.5 text-dim hover:bg-selection hover:text-fg'
  const digit = 'font-mono text-xs text-magenta'
  /** A tab's number is its key after the leader. */
  const chord = (title: string, n: number) => (n < 10 ? tip(title, [`${KEY.leader[0]} ${n}`]) : title)
  /** Ctrl-W closes the tab in front, and no other. */
  const shut = (id: number) => (id === active ? tip(capital(t('close')), KEY.closeTab) : undefined)
</script>

<div
  role="tablist"
  aria-label={t('sessions')}
  class="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-edge bg-panel px-2.5 py-2"
>
  <span class={pill}>
    <button type="button" role="tab" aria-selected={active === null} title={chord(t('hosts'), 0)} onclick={() => onpick(null)} class="{tab} pr-3.5">
      <span class={digit}>0</span>
      {t('hosts')}
    </button>
  </span>
  {#each sessions as session, at (session.id)}
    <span class={pill} data-broadcast={session.broadcast || undefined}>
      <button
        type="button"
        role="tab"
        aria-selected={session.id === active}
        title={chord(session.title, at + 1)}
        onclick={() => onpick(session.id)}
        class="{tab} pr-1"
      >
        <SessionDot {session} />
        <span class={digit}>{at + 1}</span>
        {session.title}
        {#if session.busy}
          <!-- Closing is at once, as in the TUI: this is the warning there is. -->
          <span class="rounded-full bg-bar px-2 text-xs font-normal text-white">{t('running')} <span class="font-mono">{session.busy}</span></span>
        {/if}
      </button>
      <button
        type="button"
        aria-label="{t('close')} {session.title}"
        title={shut(session.id)}
        onclick={() => onclose(session.id)}
        class={cross}
      >
        ×
      </button>
    </span>
  {/each}
  {#each files as view, at (view.id)}
    <span class={pill}>
      <button
        type="button"
        role="tab"
        aria-selected={view.id === active}
        title={chord(`${t('files')} ${view.host}`, sessions.length + at + 1)}
        onclick={() => onpick(view.id)}
        class="{tab} pr-1"
      >
        <span class={digit}>{sessions.length + at + 1}</span>
        <span class="text-cyan">{t('files')}</span>
        {view.host}
      </button>
      <button type="button" aria-label="{t('close')} {t('files')} {view.host}" title={shut(view.id)} onclick={() => onclose(view.id)} class={cross}>
        ×
      </button>
    </span>
  {/each}
</div>
