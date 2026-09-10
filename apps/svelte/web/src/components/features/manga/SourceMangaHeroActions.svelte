<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { t } from "$lib/i18n";
  import { Check, Loader2 } from "@lucide/svelte";

  let {
    addedToLibrary,
    addingToLibrary,
    categories,
    categoryLoading,
    selectedCategory,
    onAddToLibrary,
    onSelectCategory,
  } = $props<{
    addedToLibrary: boolean;
    addingToLibrary: boolean;
    categories: string[];
    categoryLoading: boolean;
    selectedCategory: string;
    onAddToLibrary: () => void;
    onSelectCategory: (value: string) => void;
  }>();
</script>

<div class="flex flex-wrap items-end gap-3">
  <div class="w-full max-w-xs">
    <label for="library-category" class="mb-2 block text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{$t("app.library.filters.category", { value: "" }).replace(": ", "")}</label>
    <Select type="single" value={selectedCategory} onValueChange={onSelectCategory}>
      <SelectTrigger id="library-category" class="h-10 w-full">
        {selectedCategory || (categoryLoading ? $t("app.manga.loadingCategories") : $t("app.manga.selectCategory"))}
      </SelectTrigger>
      <SelectContent>
        {#each categories as category (category)}
          <SelectItem value={category}>{category}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    {#if categories.length === 0}
      <p class="mt-2 text-xs text-muted-foreground">{$t("app.manga.categoryFirst")}</p>
    {/if}
  </div>
  <Button
    size="lg"
    onclick={onAddToLibrary}
    disabled={addingToLibrary || addedToLibrary || !selectedCategory}
    variant={addedToLibrary ? "outline" : "default"}
    class={addedToLibrary ? "border-primary/30 text-primary hover:bg-primary/10 hover:text-primary" : ""}
  >
    {#if addedToLibrary}
      <Check class="w-4 h-4 mr-2" /> {$t("app.manga.inLibrary")}
    {:else if addingToLibrary}
      <Loader2 class="w-4 h-4 mr-2 animate-spin" /> {$t("app.manga.adding")}
    {:else}
      {$t("app.manga.addToLibrary")}
    {/if}
  </Button>
</div>
