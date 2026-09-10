<script lang="ts">
  import { page } from "$app/state";
  import {
    createTable,
    tableFeatures,
    type ColumnDef,
    type Header,
    type Row,
  } from "@tanstack/svelte-table";
  import { Activity, Database, Server, Shield } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import { TableCell, TableRow } from "$lib/ui/table";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import type { AdminSummary } from "./dev-api";

  interface RuntimeActivityRow {
    detail: string;
    id: string;
    ok: boolean;
    status: string;
    title: string;
  }

  const {
    activeDownloads,
    enabledSources,
    failedDownloads,
    healthOk,
    loading,
    settingsCount,
    summary,
  }: {
    activeDownloads: number;
    enabledSources: number;
    failedDownloads: number;
    healthOk: boolean;
    loading: boolean;
    settingsCount: number;
    summary: AdminSummary;
  } = $props();

  const buildLabel = $derived(
    summary.info?.commit_short_hash
      ? `${summary.info.version ?? $t("app.dev.admin.devBuild")} (${summary.info.commit_short_hash})`
      : (summary.info?.version ?? $t("app.dev.admin.unknown")),
  );
  const runtimeActivityRows = $derived(getRuntimeActivityRows());

  function getRuntimeActivityRows(): RuntimeActivityRow[] {
    const rows: RuntimeActivityRow[] = [
      {
        detail: page.url.pathname,
        id: "current-route",
        ok: true,
        status: $t("app.dev.admin.live"),
        title: $t("app.dev.admin.currentRoute"),
      },
    ];

    const checks = summary.health?.checks ?? [];

    if (checks.length > 0) {
      for (const [index, check] of checks.entries()) {
        const component = check.component ?? `check-${index + 1}`;
        rows.push({
          detail: formatValue(check.detail),
          id: `${component}-${index}`,
          ok: check.ok !== false,
          status: check.ok === false ? "Fail" : "OK",
          title: formatComponentName(component),
        });
      }

      return rows;
    }

    rows.push({
      detail: loading ? $t("app.common.loading") : $t("app.dev.admin.noCheckDetails"),
      id: "backend-checks",
      ok: healthOk,
      status: healthOk ? $t("app.dev.admin.ok") : $t("app.dev.admin.unavailable"),
      title: $t("app.dev.admin.backendChecks"),
    });

    return rows;
  }

  const runtimeTableFeatures = tableFeatures({});
  type RuntimeTableHeader = Header<typeof runtimeTableFeatures, RuntimeActivityRow>;
  type RuntimeTableRow = Row<typeof runtimeTableFeatures, RuntimeActivityRow>;

  const runtimeColumns: ColumnDef<typeof runtimeTableFeatures, RuntimeActivityRow>[] = [
    {
      accessorKey: "title",
      header: $t("app.dev.admin.check"),
    },
    {
      accessorKey: "status",
      header: $t("app.dev.admin.status"),
    },
    {
      accessorKey: "detail",
      header: $t("app.dev.admin.detail"),
    },
  ];

  const runtimeTable = createTable({
    _features: runtimeTableFeatures,
    _rowModels: {},
    get data() {
      return runtimeActivityRows;
    },
    columns: runtimeColumns,
    getRowId: (row: RuntimeActivityRow) => row.id,
  });
  const runtimeRows = $derived(runtimeTable.getRowModel().rows);

  function formatValue(value: string | number | boolean | null | undefined): string {
    if (value === null || value === undefined || value === "") {
      return $t("app.dev.admin.notSet");
    }

    return String(value);
  }

  function formatComponentName(component: string): string {
    return component
      .split(/[_-]/)
      .filter(Boolean)
      .map((part) => part[0].toUpperCase() + part.slice(1))
      .join(" ");
  }
</script>

{#snippet runtimeHeader(header: RuntimeTableHeader)}
  {header.column.columnDef.header}
{/snippet}

{#snippet runtimeRow(_row: RuntimeTableRow, activity: RuntimeActivityRow)}
  <TableRow>
    <TableCell>{activity.title}</TableCell>
    <TableCell>
      <Badge variant={activity.ok ? "success" : "destructive"}>{activity.status}</Badge>
    </TableCell>
    <TableCell class={activity.id === "current-route" ? "font-mono text-xs text-muted-foreground" : "text-muted-foreground"}>
      {activity.detail}
    </TableCell>
  </TableRow>
{/snippet}

<div class="space-y-6">
  <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
    <div class="flex items-center gap-4 border border-border bg-card p-5">
      <div class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Server class="size-5" />
      </div>
      <div class="min-w-0">
        <p class="truncate text-2xl font-bold">{healthOk ? $t("app.about.status.online") : $t("app.about.status.offline")}</p>
        <p class="text-sm text-muted-foreground">{$t("app.dev.admin.backendHealth")}</p>
      </div>
      <Badge variant={healthOk ? "success" : "destructive"} class="ml-auto">{healthOk ? $t("app.dev.admin.ok") : $t("app.dev.admin.down")}</Badge>
    </div>

    <div class="flex items-center gap-4 border border-border bg-card p-5">
      <div class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Database class="size-5" />
      </div>
      <div>
        <p class="text-2xl font-bold">{summary.libraryCount}</p>
        <p class="text-sm text-muted-foreground">{$t("app.dev.admin.libraryItems")}</p>
      </div>
    </div>

    <div class="flex items-center gap-4 border border-border bg-card p-5">
      <div class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Activity class="size-5" />
      </div>
      <div>
        <p class="text-2xl font-bold">{activeDownloads}</p>
        <p class="text-sm text-muted-foreground">{$t("app.dev.admin.activeDownloads")}</p>
      </div>
      {#if failedDownloads > 0}
        <Badge variant="destructive" class="ml-auto">{$t("app.dev.admin.failedCount", { count: failedDownloads })}</Badge>
      {/if}
    </div>

    <div class="flex items-center gap-4 border border-border bg-card p-5">
      <div class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Shield class="size-5" />
      </div>
      <div>
        <p class="text-2xl font-bold">{enabledSources}/{summary.sources.length}</p>
        <p class="text-sm text-muted-foreground">{$t("app.dev.admin.enabledSources")}</p>
      </div>
    </div>
  </div>

  <div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
    <section class="border border-border bg-card">
      <header class="border-b border-border px-4 py-3">
        <h3 class="font-semibold">{$t("app.dev.admin.runtimeActivity")}</h3>
        <p class="text-sm text-muted-foreground">{$t("app.dev.admin.runtimeActivityDescription")}</p>
      </header>
      <VirtualizedTanStackTable
        table={runtimeTable}
        rows={runtimeRows}
        columnCount={runtimeColumns.length}
        estimateSize={48}
        overscan={6}
        containerClass="overflow-x-auto p-4"
        showFooter={false}
        header={runtimeHeader}
        row={runtimeRow}
      />
    </section>

    <section class="border border-border bg-card">
      <header class="border-b border-border px-4 py-3">
        <h3 class="font-semibold">{$t("app.dev.admin.build")}</h3>
        <p class="text-sm text-muted-foreground">{buildLabel}</p>
      </header>
      <dl class="grid gap-3 p-4 text-sm">
        <div class="flex justify-between gap-3">
          <dt class="text-muted-foreground">{$t("app.about.details.target")}</dt>
          <dd class="truncate text-right">{formatValue(summary.info?.build_target)}</dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="text-muted-foreground">{$t("app.dev.admin.channel")}</dt>
          <dd class="truncate text-right">{formatValue(summary.info?.build_channel)}</dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="text-muted-foreground">{$t("app.about.details.branch")}</dt>
          <dd class="truncate text-right">{formatValue(summary.info?.branch)}</dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="text-muted-foreground">{$t("app.dev.admin.gitClean")}</dt>
          <dd class="truncate text-right">{formatValue(summary.info?.git_clean)}</dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="text-muted-foreground">{$t("app.nav.settings")}</dt>
          <dd>{settingsCount}</dd>
        </div>
      </dl>
    </section>
  </div>
</div>
