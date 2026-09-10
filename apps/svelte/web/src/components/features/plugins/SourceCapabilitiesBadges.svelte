<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";

  const { capabilities }: { capabilities: string[] } = $props();
</script>

<div class="flex flex-wrap gap-1">
  {#if capabilities.length === 0}
    <Badge variant="outline">{$t("app.sources.noCapabilities")}</Badge>
  {:else}
    {#each capabilities as capability (capability)}
      <Badge variant="outline">{capability}</Badge>
    {/each}
  {/if}
</div>
