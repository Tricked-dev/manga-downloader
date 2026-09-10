<script lang="ts">
  import { cn } from "../utils";

  let {
    message,
    class: className = "",
  }: {
    class?: string;
    message: string;
  } = $props();
</script>

<div class={cn("panel p-6 text-sm text-muted-foreground", className)}>
  {message}
</div>
