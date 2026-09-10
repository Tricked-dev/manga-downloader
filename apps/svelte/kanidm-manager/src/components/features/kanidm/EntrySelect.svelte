<script lang="ts">
  import type { KanidmEntry } from "$lib/kanidm/types";
  import { appDisplayName, entryName } from "$lib/kanidm/utils";

  let {
    entries,
    placeholder = "Select entry",
    showName = false,
    value = $bindable(""),
  }: {
    entries: KanidmEntry[];
    placeholder?: string;
    showName?: boolean;
    value: string;
  } = $props();
</script>

<select class="control" bind:value>
  {#if placeholder}
    <option value="">{placeholder}</option>
  {/if}
  {#each entries as entry (entryName(entry))}
    <option value={entryName(entry)}>
      {appDisplayName(entry)}{showName ? ` (${entryName(entry)})` : ""}
    </option>
  {/each}
</select>
