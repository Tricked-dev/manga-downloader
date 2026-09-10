<script lang="ts">
  import AppLogo from "$components/layout/AppLogo.svelte";

  const { children } = $props();
</script>

<div class="flex min-h-screen items-center justify-center px-4 py-10">
  <div class="w-full max-w-sm">
    <div class="mb-6 flex justify-center">
      <AppLogo compact />
    </div>
    {@render children()}
  </div>
</div>
