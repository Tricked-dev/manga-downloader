<script lang="ts">
  import { Info } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { Popover, PopoverContent, PopoverTrigger } from "$lib/ui/popover";

  const { content, align = "right" }: { content: string; align?: "left" | "right" } = $props();
  const contentAlign = $derived(align === "left" ? "end" : "start");
</script>

<Popover>
  <PopoverTrigger
    class="inline-flex size-4 items-center justify-center rounded-full text-muted-foreground transition-colors hover:text-foreground focus-visible:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
    type="button"
    openOnHover={true}
    openDelay={120}
    closeDelay={80}
    aria-label={$t("app.common.showSettingInfo")}
  >
    <Info class="size-3.5" />
  </PopoverTrigger>
  <PopoverContent
    align={contentAlign}
    sideOffset={8}
    class="w-64 max-w-[min(20rem,calc(100vw-2rem))] whitespace-pre-line border border-border px-3 py-2 text-xs leading-5"
  >
    {content}
  </PopoverContent>
</Popover>
