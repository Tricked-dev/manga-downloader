<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import UpdatesFeedItem from "$components/features/updates/UpdatesFeedItem.svelte";
  import { t } from "$lib/i18n";
  import type { UpdateFeedItem, UpdatesSection } from "$lib/features/updates/types";

  let {
    sections,
    itemCount,
    getCategoryLabel,
  } = $props<{
    sections: UpdatesSection[];
    itemCount: number;
    getCategoryLabel: (item: UpdateFeedItem) => string;
  }>();
</script>

<Card class="gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.updates.recentReleases")}</CardTitle>
    <CardDescription>{$t("app.updates.recentReleasesDescription", { count: itemCount })}</CardDescription>
  </CardHeader>
  <CardContent class="p-0">
    {#each sections as section, sectionIndex (section.label)}
      <section class={sectionIndex === 0 ? "" : "border-t border-border/70"}>
        <div class="px-4 pb-2 pt-4">
          <h2 class="text-2xl font-semibold tracking-tight">{section.label}</h2>
        </div>

        <div class="space-y-2 px-3 pb-3">
          {#each section.items as item (item.chapter.id)}
            <UpdatesFeedItem {item} categoryLabel={getCategoryLabel(item)} />
          {/each}
        </div>
      </section>
    {/each}
  </CardContent>
</Card>
