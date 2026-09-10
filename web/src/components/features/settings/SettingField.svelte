<script lang="ts">
  import type { Snippet } from "svelte";

  import FormField from "@manga-server/ui/components/form-field";
  import SettingInfoHint from "./SettingInfoHint.svelte";

  const {
    id,
    label,
    hint = "",
    description = "",
    children,
    class: className = "",
    labelClass = "",
  } = $props<{
    id: string;
    label: string;
    hint?: string;
    description?: string;
    children?: Snippet;
    class?: string;
    labelClass?: string;
  }>();
</script>

<FormField {id} {label} {description} class={className} {labelClass}>
  {#snippet aside()}
    {#if hint}
      <SettingInfoHint content={hint} />
    {/if}
  {/snippet}
  {#if children}
    {@render children()}
  {/if}
</FormField>
