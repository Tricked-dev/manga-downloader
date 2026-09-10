<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import MangaHero from "$components/manga/MangaHero.svelte";
  import PublicShareControls from "$components/layout/PublicShareControls.svelte";
  import { t } from "$lib/i18n";
  import type { LibraryManga } from "$lib/types";
  import type { PublicShareAuthState } from "$lib/public-share";
  import { Trash2 } from "@lucide/svelte";

  let {
    availableCategories,
    categorySaveMessage,
    comicInfoRows,
    effectiveSelectedCategory,
    manga,
    publicShareAuth = null,
    publicView = false,
    removeMessage,
    removeMessageIsError,
    removePending,
    savingCategory,
    settingsPending,
    onRemove,
    onSaveCategory,
    onSelectCategory,
  } = $props<{
    availableCategories: string[];
    categorySaveMessage: string;
    comicInfoRows: [string, string][];
    effectiveSelectedCategory: string;
    manga: LibraryManga;
    publicShareAuth?: PublicShareAuthState | null;
    publicView?: boolean;
    removeMessage: string;
    removeMessageIsError: boolean;
    removePending: boolean;
    savingCategory: boolean;
    settingsPending: boolean;
    onRemove: () => void;
    onSaveCategory: () => void;
    onSelectCategory: (value: string) => void;
  }>();
</script>

{#snippet shareControls()}
  <PublicShareControls auth={publicShareAuth} showPublicViewBanner={false} inline />
{/snippet}

{#snippet detailsContent()}
  {#if comicInfoRows.length > 0}
    <dl class="grid gap-x-8 gap-y-3 sm:grid-cols-2 xl:grid-cols-3">
      {#each comicInfoRows as [label, value] (label)}
        <div class={label === $t("app.libraryDetail.genre") ? "min-w-0 sm:col-span-2 xl:col-span-3" : "min-w-0"}>
          <dt class="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">{label}</dt>
          <dd class="truncate text-sm leading-6 text-foreground">{value}</dd>
        </div>
      {/each}
    </dl>
  {/if}
{/snippet}

{#snippet actionsContent()}
  <div class="max-w-sm space-y-2">
    <label for="category" class="block text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{$t("app.libraryDetail.category")}</label>
    <div class="flex gap-2">
      <div class="flex-1">
        <Select type="single" value={effectiveSelectedCategory} onValueChange={onSelectCategory}>
          <SelectTrigger id="category" class="h-10 w-full">
            {effectiveSelectedCategory || (settingsPending ? $t("app.manga.loadingCategories") : $t("app.manga.selectCategory"))}
          </SelectTrigger>
          <SelectContent>
            {#each availableCategories as category (category)}
              <SelectItem value={category}>{category}</SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </div>
      <Button onclick={onSaveCategory} disabled={savingCategory || !effectiveSelectedCategory}>
        {savingCategory ? $t("app.libraryDetail.saving") : $t("app.libraryDetail.save")}
      </Button>
    </div>
    {#if availableCategories.length === 0}
      <p class="text-xs text-muted-foreground">{$t("app.manga.categoryFirst")}</p>
    {/if}
    {#if categorySaveMessage}
      <p class="text-xs text-muted-foreground">{categorySaveMessage}</p>
    {/if}
    <div class="mt-4 border-t border-border pt-4">
      <p class="mb-2 block text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{$t("app.libraryDetail.dangerZone")}</p>
      <Button
        variant="destructive"
        size="sm"
        disabled={removePending}
        onclick={onRemove}
      >
        <Trash2 class="h-4 w-4" />
        {removePending ? $t("app.libraryDetail.removing") : $t("app.libraryDetail.removeFromLibrary")}
      </Button>
      {#if removeMessage}
        <p class={`mt-2 text-xs ${removeMessageIsError ? "text-destructive" : "text-muted-foreground"}`}>{removeMessage}</p>
      {/if}
    </div>
  </div>
{/snippet}

<MangaHero
  title={manga.title}
  coverUrl={manga.cover_proxy_url ?? manga.cover_url}
  coverBaseUrl={manga.source_base_url}
  author={manga.author}
  status={manga.status}
  description={manga.comic_info?.summary?.trim() || manga.description}
  headerActions={publicView ? undefined : shareControls}
  backHref="/"
  backLabel={$t("app.libraryDetail.backToLibrary")}
  details={detailsContent}
  actions={publicView ? undefined : actionsContent}
/>
