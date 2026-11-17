<script lang="ts">
  import { t } from './i18n.svelte'
  import SessionDot from './SessionDot.svelte'

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
    /** The session in front; `null` = the dashboard. */
    active: number | null
    onpick: (id: number | null) => void
    onclose: (id: number) => void
  }
  let { sessions, active, onpick, onclose }: Props = $props()

  // The TUI's tab: a pill with a rounded top, filled when it is in front.
  // Broadcast takes the colours, so the tab in front is also bold, and only it has the full border.
  const pill =
    'flex shrink-0 items-center rounded-t-lg border border-b-0 border-transparent text-white hover:text-fg has-[[aria-selected=true]]:border-edge has-[[aria-selected=true]]:bg-bg has-[[aria-selected=true]]:font-bold has-[[aria-selected=true]]:text-fg data-[broadcast]:border-yellow/40! data-[broadcast]:text-yellow! data-[broadcast]:has-[[aria-selected=true]]:border-yellow!'
  const tab = 'flex cursor-pointer items-center gap-1.5 py-1 pl-3'
</script>

<div
  role="tablist"
  aria-label={t('sessions')}
  class="flex items-end gap-1 overflow-x-auto border-b border-edge bg-black px-2 pt-1.5"
>
  <span class={pill}>
    <button type="button" role="tab" aria-selected={active === null} onclick={() => onpick(null)} class="{tab} pr-3">
      <span class="text-magenta">0</span>
      {t('hosts')}
    </button>
  </span>
  {#each sessions as session, at (session.id)}
    <span class={pill} data-broadcast={session.broadcast || undefined}>
      <button
        type="button"
        role="tab"
        aria-selected={session.id === active}
        onclick={() => onpick(session.id)}
        class="{tab} pr-1"
      >
        <SessionDot {session} />
        <span class="text-magenta">{at + 1}</span>
        {session.title}
        {#if session.busy}
          <!-- Closing is at once, as in the TUI: this is the warning there is. -->
          <span class="rounded-full bg-bar px-1.5 text-xs text-yellow">{t('running')} {session.busy}</span>
        {/if}
      </button>
      <button
        type="button"
        aria-label="{t('close')} {session.title}"
        onclick={() => onclose(session.id)}
        class="cursor-pointer rounded px-1.5 text-dim hover:bg-selection hover:text-fg"
      >
        ×
      </button>
    </span>
  {/each}
</div>
