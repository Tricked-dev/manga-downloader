<script lang="ts">
  import AppShell from "$components/layout/AppShell.svelte";
  import type { LayoutData } from "./$types";

  const { children, data } = $props<{
    children: () => unknown;
    data: LayoutData;
  }>();
</script>

<AppShell auth={data.auth}>
  {@render children()}
</AppShell>
