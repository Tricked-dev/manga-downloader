<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import type { PluginArtifact } from "$lib/types";

  const {
    artifacts = [],
    loadingArtifacts = false,
    deleting = false,
    onDelete,
  } = $props<{
    artifacts?: PluginArtifact[];
    loadingArtifacts?: boolean;
    deleting?: boolean;
    onDelete: () => void;
  }>();
</script>

<div class="mt-3 space-y-3 rounded-md border border-border/70 px-3 py-3">
  <div class="flex items-center justify-between gap-3">
    <div>
      <p class="text-sm font-medium text-foreground">{$t("app.sources.artifactHistory")}</p>
      <p class="text-xs text-muted-foreground">{$t("app.sources.artifactHistoryDescription")}</p>
    </div>
    <Button
      variant="destructive"
      size="sm"
      disabled={deleting}
      onclick={onDelete}
    >
      {deleting ? $t("app.sources.deleting") : $t("app.sources.deletePlugin")}
    </Button>
  </div>

  {#if loadingArtifacts}
    <p class="text-xs text-muted-foreground">{$t("app.sources.loadingArtifacts")}</p>
  {:else if artifacts.length === 0}
    <p class="text-xs text-muted-foreground">{$t("app.sources.noArtifactHistory")}</p>
  {:else}
    <div class="space-y-2">
      {#each artifacts as artifact (artifact.id)}
        <div class="rounded-md border border-border/70 px-3 py-2">
          <div class="flex flex-wrap items-center gap-2">
            <p class="text-sm font-medium">{artifact.plugin_version}</p>
            {#if artifact.is_active}
              <Badge variant="secondary">{$t("app.sources.active")}</Badge>
            {/if}
          </div>
          <p class="mt-1 text-xs text-muted-foreground">{$t("app.sources.apiVersion", { version: artifact.plugin_api_version })}</p>
          <p class="text-xs text-muted-foreground">{artifact.artifact_path}</p>
          <p class="text-xs text-muted-foreground">{$t("app.sources.installed", { date: artifact.installed_at })}</p>
          {#if artifact.replaced_at}
            <p class="text-xs text-muted-foreground">{$t("app.sources.replaced", { date: artifact.replaced_at })}</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
