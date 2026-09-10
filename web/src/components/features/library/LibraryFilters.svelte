<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { t } from "$lib/i18n";
  import { CircleDot, Languages, Library as LibraryIcon, Search, Star, Tags, X } from "@lucide/svelte";

  let {
    searchQuery,
    selectedCategory,
    selectedAgeRating,
    selectedGenre,
    selectedLanguage,
    selectedStatus,
    categoryOptions,
    ageRatingOptions,
    genreOptions,
    languageOptions,
    statusOptions,
    activeFilters,
    hasActiveFilters,
    onSearchInput,
    onSelectCategory,
    onSelectStatus,
    onSelectGenre,
    onSelectLanguage,
    onSelectAgeRating,
    onReset,
  } = $props<{
    searchQuery: string;
    selectedCategory: string;
    selectedAgeRating: string;
    selectedGenre: string;
    selectedLanguage: string;
    selectedStatus: string;
    categoryOptions: string[];
    ageRatingOptions: string[];
    genreOptions: string[];
    languageOptions: string[];
    statusOptions: string[];
    activeFilters: string[];
    hasActiveFilters: boolean;
    onSearchInput: (event: Event) => void;
    onSelectCategory: (value: string) => void;
    onSelectStatus: (value: string) => void;
    onSelectGenre: (value: string) => void;
    onSelectLanguage: (value: string) => void;
    onSelectAgeRating: (value: string) => void;
    onReset: () => void;
  }>();

  function optionLabel(value: string, fallback: string) {
    return value === "all" ? fallback : value;
  }

  function filterTriggerClass(active: boolean) {
    return `size-9 justify-center p-0 [&>svg:last-child]:hidden ${
      active ? "border-primary/60 bg-primary/10 text-primary" : "text-muted-foreground"
    }`;
  }
</script>

<div class="space-y-2">
  <div class="grid w-full min-w-0 grid-cols-5 gap-2 md:grid-cols-[minmax(18rem,1fr)_repeat(5,2.25rem)]">
    <div class="relative col-span-5 md:col-span-1">
      <Search class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        placeholder={$t("app.library.search")}
        value={searchQuery}
        oninput={onSearchInput}
        aria-label={$t("app.library.searchAria")}
        class="h-9 pl-9"
      />
    </div>
    <Select type="single" value={selectedCategory} onValueChange={onSelectCategory}>
      <SelectTrigger
        class={filterTriggerClass(selectedCategory !== "all")}
        aria-label={$t("app.library.category", { value: selectedCategory === "all" ? $t("app.library.allCategories") : selectedCategory })}
        title={selectedCategory === "all" ? $t("app.library.allCategories") : selectedCategory}
      >
        <LibraryIcon class="size-4" />
      </SelectTrigger>
      <SelectContent class="min-w-40">
        {#each categoryOptions as category (category)}
          <SelectItem value={category}>{category === "all" ? $t("app.library.allCategories") : category}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    <Select type="single" value={selectedStatus} onValueChange={onSelectStatus}>
      <SelectTrigger
        class={filterTriggerClass(selectedStatus !== "all")}
        aria-label={$t("app.library.filters.status", { value: optionLabel(selectedStatus, $t("app.library.allStatuses")) })}
        title={optionLabel(selectedStatus, $t("app.library.allStatuses"))}
      >
        <CircleDot class="size-4" />
      </SelectTrigger>
      <SelectContent>
        {#each statusOptions as status (status)}
          <SelectItem value={status}>{optionLabel(status, $t("app.library.allStatuses"))}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    <Select type="single" value={selectedGenre} onValueChange={onSelectGenre}>
      <SelectTrigger
        class={filterTriggerClass(selectedGenre !== "all")}
        aria-label={$t("app.library.filters.genre", { value: optionLabel(selectedGenre, $t("app.library.allGenres")) })}
        title={optionLabel(selectedGenre, $t("app.library.allGenres"))}
      >
        <Tags class="size-4" />
      </SelectTrigger>
      <SelectContent>
        {#each genreOptions as genre (genre)}
          <SelectItem value={genre}>{optionLabel(genre, $t("app.library.allGenres"))}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    <Select type="single" value={selectedLanguage} onValueChange={onSelectLanguage}>
      <SelectTrigger
        class={filterTriggerClass(selectedLanguage !== "all")}
        aria-label={$t("app.library.filters.language", { value: optionLabel(selectedLanguage, $t("app.library.allLanguages")) })}
        title={optionLabel(selectedLanguage, $t("app.library.allLanguages"))}
      >
        <Languages class="size-4" />
      </SelectTrigger>
      <SelectContent>
        {#each languageOptions as language (language)}
          <SelectItem value={language}>{optionLabel(language, $t("app.library.allLanguages"))}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    <Select type="single" value={selectedAgeRating} onValueChange={onSelectAgeRating}>
      <SelectTrigger
        class={filterTriggerClass(selectedAgeRating !== "all")}
        aria-label={$t("app.library.filters.rating", { value: optionLabel(selectedAgeRating, $t("app.library.allRatings")) })}
        title={optionLabel(selectedAgeRating, $t("app.library.allRatings"))}
      >
        <Star class="size-4" />
      </SelectTrigger>
      <SelectContent>
        {#each ageRatingOptions as ageRating (ageRating)}
          <SelectItem value={ageRating}>{optionLabel(ageRating, $t("app.library.allRatings"))}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
  </div>
  {#if hasActiveFilters}
    <div class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
      <span>{$t(activeFilters.length === 1 ? "app.library.activeFilter" : "app.library.activeFilters", { count: activeFilters.length })}</span>
      {#each activeFilters as filter (filter)}
        <Badge variant="secondary" class="max-w-full truncate text-[10px]">{filter}</Badge>
      {/each}
      <Button variant="ghost" size="sm" class="h-7 gap-1 px-2 text-xs" onclick={onReset}>
        <X class="size-3.5" />
        {$t("app.actions.reset")}
      </Button>
    </div>
  {/if}
</div>
