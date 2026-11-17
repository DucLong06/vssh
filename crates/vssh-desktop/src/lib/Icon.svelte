<script lang="ts" module>
  // Concept A's icons (docs/mockups/concept-a.html), drawn here: 24 units, stroked, no fill.
  const ICONS = {
    grid: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>',
    server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>',
    term: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M7 9l3 3-3 3M12.5 15H17"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    code: '<path d="M8.5 7 4 12l4.5 5M15.5 7 20 12l-4.5 5M13.5 5l-3 14"/>',
    fwd: '<path d="M4 8h13M13.5 4.5 17 8l-3.5 3.5M20 16H7M10.5 12.5 7 16l3.5 3.5"/>',
    lock: '<rect x="5" y="10.5" width="14" height="9.5" rx="2"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
    eye: '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/>',
    // The eye, struck through.
    blind: '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/><path d="M4 4l16 16"/>',
    moon: '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>',
    // A keyboard: the way to the cheatsheet.
    keys: '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6.5 10h.01M10.2 10h.01M13.8 10h.01M17.5 10h.01M6.5 14h.01M17.5 14h.01M9.8 14h4.4"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  }
  export type IconName = keyof typeof ICONS
</script>

<script lang="ts">
  let { name, class: extra = '' }: { name: IconName; class?: string } = $props()
</script>

<svg
  viewBox="0 0 24 24"
  aria-hidden="true"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  class="size-[18px] shrink-0 fill-none stroke-current {extra}"
>
  <!-- Constants of this file, nothing a person typed. -->
  {@html ICONS[name]}
</svg>
