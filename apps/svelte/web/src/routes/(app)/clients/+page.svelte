<script lang="ts">
  import PageHeader from "@manga-server/ui/components/page-header";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { clientPackages } from "$lib/features/clients/clients";
  import type { ClientDownload } from "$lib/features/clients/clients";
  import { fetchClientPackage, saveBlobAsFile } from "$lib/features/clients/download-package";
  import { t } from "$lib/i18n";
  import { MonitorSmartphone, Package } from "@lucide/svelte";
  import { SvelteMap } from "svelte/reactivity";
  import ClientDownloadButton from "./ClientDownloadButton.svelte";

  const pendingDownloads = new SvelteMap<string, boolean>();
  const downloadErrors = new SvelteMap<string, string>();

  async function downloadPackage(download: ClientDownload) {
    if (pendingDownloads.get(download.href)) {
      return;
    }

    pendingDownloads.set(download.href, true);
    downloadErrors.delete(download.href);

    try {
      const blob = await fetchClientPackage(download);
      saveBlobAsFile(blob, download.filename);
    } catch (error) {
      downloadErrors.set(
        download.href,
        error instanceof Error ? error.message : $t("app.clients.downloadFailed"),
      );
    } finally {
      pendingDownloads.delete(download.href);
    }
  }
</script>

<svelte:head>
  <title>{$t("app.clients.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-5">
  <PageHeader title={$t("app.clients.title")} description={$t("app.clients.description")}>
    {#snippet actions()}
      <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
        <MonitorSmartphone class="h-3.5 w-3.5 text-primary" />
        <span>{clientPackages.length}</span>
        <span class="text-muted-foreground">{$t("app.clients.available")}</span>
      </Badge>
    {/snippet}
  </PageHeader>

  <Card class="gap-0">
    <CardHeader class="border-b border-border/70 py-4">
      <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <CardTitle>{$t("app.clients.packages")}</CardTitle>
          <CardDescription>{$t("app.clients.packagesDescription")}</CardDescription>
        </div>
        <p class="text-xs text-muted-foreground">{$t("app.clients.ready", { count: clientPackages.length })}</p>
      </div>
    </CardHeader>
    <CardContent class="p-0">
      <div class="divide-y divide-border/70">
        {#each clientPackages as client (client.id)}
          <div class="space-y-4 p-4">
            <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
              <div class="min-w-0">
                <h2 class="flex items-center gap-2 text-sm font-medium text-foreground">
                  <MonitorSmartphone class="h-4 w-4 text-primary" />
                  {client.name}
                </h2>
                <p class="mt-1 text-sm text-muted-foreground">{$t(client.descriptionKey)}</p>
              </div>
              <Badge variant="outline" class="w-fit shrink-0">{client.platform}</Badge>
            </div>

            <div class="divide-y divide-border/70 border border-border/70">
              {#each client.downloads as download (download.href)}
                <div class="flex flex-col gap-4 p-4 sm:flex-row sm:items-center sm:justify-between">
                  <div class="min-w-0">
                    <p class="flex items-center gap-2 text-sm font-medium text-foreground">
                      <Package class="h-4 w-4 text-muted-foreground" />
                      {$t(download.descriptionKey)}
                    </p>
                    <p class="mt-1 break-all font-mono text-xs text-muted-foreground">
                      {download.filename}
                    </p>
                  </div>

                  <div class="flex flex-col items-start gap-2 sm:items-end">
                    <ClientDownloadButton download={download} downloading={pendingDownloads.get(download.href)} onDownload={downloadPackage} />

                    {#if downloadErrors.get(download.href)}
                      <p class="max-w-sm text-left text-xs text-destructive sm:text-right">
                        {downloadErrors.get(download.href)}
                      </p>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </CardContent>
  </Card>
</div>
