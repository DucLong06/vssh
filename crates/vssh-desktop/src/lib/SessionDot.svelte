<script lang="ts">
  import { t } from './i18n.svelte'
  import type { Tab } from './Tabs.svelte'

  /** A session's state as a dot, wherever the session is listed. */
  let { session }: { session: Tab } = $props()

  const said = $derived({ connecting: t('connecting'), up: t('connected'), closed: t('disconnected') })
  const tone = { connecting: 'text-dim', up: 'text-green', closed: 'text-red' }
</script>

<span role="img" aria-label={said[session.state]} class={tone[session.state]}>
  {session.state === 'connecting' ? '○' : '●'}
</span>
