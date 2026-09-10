<script lang="ts">
  import KanidmManager from "$components/features/kanidm/KanidmManager.svelte";

  let { data } = $props();
</script>

<svelte:head>
  <title>Kanidm Manager | Manga Server</title>
  <meta name="description" content="Manage Kanidm OAuth2 applications for Manga Server Better Auth." />
</svelte:head>

<KanidmManager {data} />
