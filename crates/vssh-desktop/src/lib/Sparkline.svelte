<script lang="ts">
  import { spark } from './spark'

  /** `points`: percentages, oldest first. The colour is the text's: `text-blue` for CPU, `text-teal` for memory. */
  let { points, class: extra = '' }: { points: number[]; class?: string } = $props()

  const path = $derived(spark(points))
</script>

<!-- A picture of what the figure beside it was a moment ago: the figure itself is said there. -->
<svg viewBox="0 0 120 32" preserveAspectRatio="none" aria-hidden="true" class="block h-[34px] {extra}">
  {#if path.area}
    <path d={path.area} fill="currentColor" opacity=".14" />
  {/if}
  {#if path.line}
    <path
      d={path.line}
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      vector-effect="non-scaling-stroke"
    />
  {/if}
</svg>
