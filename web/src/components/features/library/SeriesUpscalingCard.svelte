<script lang="ts">
  import { onMount } from "svelte";
  import { request } from "@manga-server/api-client";
  import { getErrorMessage } from "$lib/utils";
  import { Card, CardContent } from "$lib/ui/card";

  let { seriesId } = $props<{ seriesId: string }>();
  type Progress = { status: string; completed_pages: number; total_pages: number; message: string; updated_at: string };
  type Snapshot = { automatic: boolean | null; enabled: boolean; chapters: { download_id: string; chapter_number: number; upscaled_at: string | null; progress: Progress | null }[] };
  let snapshot = $state<Snapshot | null>(null);
  let error = $state("");
  let saving = $state(false);
  const endpoint = $derived(`/v1/library/${encodeURIComponent(seriesId)}/upscaling`);

  onMount(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    async function refresh() {
      try {
        const result = await request<Snapshot>(endpoint);
        if (!disposed && !saving) { snapshot = result; error = ""; }
      } catch (cause) { if (!disposed) error = getErrorMessage(cause); }
      finally { if (!disposed) timer = setTimeout(refresh, 3000); }
    }
    void refresh();
    return () => { disposed = true; clearTimeout(timer); };
  });

  async function save(value: string) {
    saving = true;
    error = "";
    try {
      await request(endpoint, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ automatic: value === "inherit" ? null : value === "true" }) });
      snapshot = await request<Snapshot>(endpoint);
    } catch (cause) { error = getErrorMessage(cause); }
    finally { saving = false; }
  }
</script>

<Card>
  <CardContent class="space-y-3 p-5">
    <h2 class="font-semibold">Series upscaling</h2>
    <label class="flex flex-wrap items-center justify-between gap-3">
      <span>Automatically upscale new downloads</span>
      <select aria-label="Series automatic upscaling" class="rounded border border-border bg-background p-2" disabled={!snapshot || saving}
        value={snapshot?.automatic == null ? "inherit" : String(snapshot.automatic)} onchange={(event) => save(event.currentTarget.value)}>
        <option value="inherit">Use global/source settings</option>
        <option value="true">On for this series</option>
        <option value="false">Off for this series</option>
      </select>
    </label>
    <p class="text-sm text-muted-foreground">On works even when global upscaling is off. For existing downloads, select chapters below and choose Upscale selected. Turning this off does not cancel queued jobs.</p>
    {#if snapshot}<p class="text-sm">Automatic upscaling: {snapshot.enabled ? "on" : "off"}</p>{/if}
    {#if error}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    {#if snapshot?.chapters.length}
      <div class="max-h-64 space-y-3 overflow-auto" aria-label="Upscale progress">
        {#each snapshot.chapters as chapter (chapter.download_id)}
          {@const progress = chapter.progress}
          <div class="text-sm">
            <div class="flex justify-between gap-3"><span>Chapter {chapter.chapter_number}</span><span>{progress?.status ?? "completed"}</span></div>
            {#if progress}
              <p class="text-xs text-muted-foreground">{progress.message}{progress.total_pages > 0 ? ` · ${progress.completed_pages}/${progress.total_pages} pages` : ""}</p>
              {#if progress.status === "running" && progress.total_pages > 0}<progress class="w-full" value={progress.completed_pages} max={progress.total_pages} aria-label={`Chapter ${chapter.chapter_number} upscale progress`}></progress>{/if}
            {/if}
          </div>
        {/each}
      </div>
    {:else if snapshot}<p class="text-sm text-muted-foreground">No upscale jobs for this series yet.</p>{/if}
  </CardContent>
</Card>
