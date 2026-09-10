<script lang="ts">
  import { Skeleton } from "$lib/ui/skeleton";

  const SUMMARY_ROWS = Array.from({ length: 4 }, (_, index) => index);
</script>

<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
  {#each SUMMARY_ROWS as index (index)}
    <Skeleton class="h-32 border border-border/70 bg-card" />
  {/each}
</div>
<Skeleton class="h-80 border border-border/70 bg-card" />
