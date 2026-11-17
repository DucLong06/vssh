<script lang="ts">
  import { t } from './i18n.svelte'
  import type { Tab } from './Tabs.svelte'

  /** A session's state as a dot, wherever the session is listed. */
  let { session }: { session: Tab } = $props()

  const said = $derived({ connecting: t('connecting'), up: t('connected'), closed: t('disconnected') })
  const tone = { connecting: 'text-dim', up: 'text-green', closed: 'text-red' }
  // Yellow is a live session that is broadcasting; a dead one is red whatever it was doing.
  const live = $derived(session.broadcast && session.state === 'up')
</script>

<span role="img" aria-label={live ? t('broadcast') : said[session.state]} class="text-[10px] {live ? 'text-yellow' : tone[session.state]}">
  {session.state === 'connecting' ? '○' : '●'}
</span>
