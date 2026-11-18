<script lang="ts">
  // The head of the dashboard: what it is, and a line about it. The way to the
  // palette and the settings are the rail's, which every view has.
  type Props = {
    title: string
    /** A line under the title: what the view holds. */
    sub?: string
  }
  let { title, sub = '' }: Props = $props()
</script>

<header class="bg-bg px-8 pt-5 pb-4">
  <h1 class="truncate text-2xl leading-tight font-semibold tracking-tight text-ink">{title}</h1>
  {#if sub}
    <p class="truncate text-[13px] text-white">{sub}</p>
  {/if}
</header>
