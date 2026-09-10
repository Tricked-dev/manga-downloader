<script lang="ts">
  import { CheckCircle2, Info, X, XCircle } from "@lucide/svelte";
  import type { Notification } from "$lib/kanidm/types";

  let {
    notifications,
    removeNotification,
  }: {
    notifications: Notification[];
    removeNotification: (id: string) => void;
  } = $props();
</script>

<div class="fixed right-4 top-4 z-50 flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-2">
  {#each notifications as notification (notification.id)}
    <div class="panel flex items-start gap-3 p-3 shadow-lg">
      {#if notification.type === "success"}
        <CheckCircle2 class="mt-0.5 size-4 text-primary" />
      {:else if notification.type === "error"}
        <XCircle class="mt-0.5 size-4 text-destructive" />
      {:else}
        <Info class="mt-0.5 size-4 text-muted-foreground" />
      {/if}
      <p class="min-w-0 flex-1 text-sm text-foreground">{notification.message}</p>
      <button
        class="button button-ghost h-6 w-6 px-0"
        type="button"
        aria-label="Dismiss notification"
        onclick={() => removeNotification(notification.id)}
      >
        <X class="size-3.5" />
      </button>
    </div>
  {/each}
</div>
