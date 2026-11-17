<script lang="ts">
  import { t } from './i18n.svelte'

  export type Tab = { id: number; title: string; state: 'connecting' | 'up' | 'closed' }
  type Props = {
    sessions: Tab[]
    /** The session in front; `null` = the dashboard. */
    active: number | null
    onpick: (id: number | null) => void
    onclose: (id: number) => void
  }
  let { sessions, active, onpick, onclose }: Props = $props()

  const said = $derived({ connecting: t('connecting'), up: t('connected'), closed: t('disconnected') })
  const tone = { connecting: 'text-dim', up: 'text-green', closed: 'text-red' }
  // The TUI's tab: a pill with a rounded top, filled when it is in front.
  const pill =
    'flex shrink-0 items-center rounded-t-lg border border-b-0 border-transparent text-white hover:text-fg has-[[aria-selected=true]]:border-edge has-[[aria-selected=true]]:bg-bg has-[[aria-selected=true]]:text-fg'
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
    <span class={pill}>
      <button
        type="button"
        role="tab"
        aria-selected={session.id === active}
        onclick={() => onpick(session.id)}
        class="{tab} pr-1"
      >
        <span role="img" aria-label={said[session.state]} class={tone[session.state]}>
          {session.state === 'connecting' ? '○' : '●'}
        </span>
        <span class="text-magenta">{at + 1}</span>
        {session.title}
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
