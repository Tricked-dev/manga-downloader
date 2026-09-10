import * as bargauge from "@grafana/grafana-foundation-sdk/bargauge";
import * as barchart from "@grafana/grafana-foundation-sdk/barchart";
import type * as cog from "@grafana/grafana-foundation-sdk/cog";
import * as dashboard from "@grafana/grafana-foundation-sdk/dashboard";
import * as gauge from "@grafana/grafana-foundation-sdk/gauge";
import * as common from "@grafana/grafana-foundation-sdk/common";
import * as logs from "@grafana/grafana-foundation-sdk/logs";
import * as piechart from "@grafana/grafana-foundation-sdk/piechart";
import * as prometheus from "@grafana/grafana-foundation-sdk/prometheus";
import * as stat from "@grafana/grafana-foundation-sdk/stat";
import * as table from "@grafana/grafana-foundation-sdk/table";
import * as tempo from "@grafana/grafana-foundation-sdk/tempo";
import * as text from "@grafana/grafana-foundation-sdk/text";
import * as timeseries from "@grafana/grafana-foundation-sdk/timeseries";

export type Grid = { x: number; y: number; w: number; h: number };
export type Builder<T> = { build(): T };

export const datasource = { type: "prometheus", uid: "${datasource}" };
export const logsDatasource = { type: "victoriametrics-logs-datasource", uid: "victorialogs" };
export const tracesDatasource = { type: "tempo", uid: "tempo" };

class RawBuilder<T> implements Builder<T> {
  constructor(private readonly value: T) {}
  build(): T {
    return this.value;
  }
}

export function raw<T>(value: T): Builder<T> {
  return new RawBuilder(value);
}

export function prom(
  expr: string,
  legendFormat = "",
  refId = "A",
  instant = false,
  format: "time_series" | "table" = "time_series",
) {
  const query = new prometheus.DataqueryBuilder()
    .expr(expr)
    .refId(refId)
    .legendFormat(legendFormat)
    .editorMode("code" as never)
    .format(format as never)
    .datasource(datasource);
  return instant ? query.instant() : query.range();
}

export function victoriaLogsStats(expr: string, legendFormat = "", refId = "A") {
  return raw({
    datasource: logsDatasource,
    expr,
    legendFormat,
    queryType: "statsRange",
    refId,
  } as unknown as cog.Dataquery);
}

export function victoriaLogsInstant(expr: string, legendFormat = "", refId = "A") {
  return raw({
    datasource: logsDatasource,
    expr,
    legendFormat,
    queryType: "instant",
    refId,
  } as unknown as cog.Dataquery);
}

export function tempoTraceql(query: string, refId = "A", limit = 50) {
  return new tempo.DataqueryBuilder()
    .refId(refId)
    .queryType(tempo.TempoQueryType.Traceql)
    .query(query)
    .limit(limit)
    .tableType(tempo.SearchTableType.Traces)
    .datasource(tracesDatasource);
}

type TraceTableOptions = {
  sortBy?: string;
  sortDesc?: boolean;
  organize?: {
    excludeByName?: Record<string, boolean>;
    indexByName?: Record<string, number>;
    renameByName?: Record<string, string>;
  };
};

export function row(id: number, title: string, y: number) {
  return new dashboard.RowBuilder(title).id(id).gridPos({ x: 0, y, w: 24, h: 1 }).collapsed(false);
}

export function statPanel(
  id: number,
  title: string,
  grid: Grid,
  expr: string,
  unit = "short",
  decimals = 1,
) {
  return new stat.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit)
    .decimals(decimals)
    .withTarget(prom(expr, title, "A", true));
}

export function logStatPanel(
  id: number,
  title: string,
  grid: Grid,
  expr: string,
  unit = "short",
  decimals = 1,
) {
  return new stat.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(logsDatasource)
    .unit(unit)
    .decimals(decimals)
    .withTarget(victoriaLogsStats(expr, title));
}

export function gaugePanel(
  id: number,
  title: string,
  grid: Grid,
  expr: string,
  unit = "percentunit",
) {
  return new gauge.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit)
    .min(0)
    .max(1)
    .decimals(2)
    .withTarget(prom(expr, title, "A", true));
}

export function timeseriesPanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  unit = "short",
  decimals?: number,
) {
  const panel = new timeseries.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit);
  if (unit === "percentunit") panel.min(0).max(1).axisSoftMin(0).axisSoftMax(1);
  if (decimals !== undefined) panel.decimals(decimals);
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function logsTimeseriesPanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  unit = "short",
  decimals?: number,
) {
  const panel = new timeseries.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(logsDatasource)
    .unit(unit);
  if (decimals !== undefined) panel.decimals(decimals);
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function tablePanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  unit = "short",
  options: { hideTime?: boolean } = {},
) {
  const panel = new table.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit)
    .showHeader(true)
    .cellHeight("sm" as never);
  panel.withTransformation({
    id: "organize",
    options: {
      excludeByName: {
        Time: options.hideTime ?? false,
        Value: false,
        __name__: true,
        instance: true,
        job: true,
      },
      indexByName: {},
      renameByName: {
        Value: "value",
      },
    },
  } as never);
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function tracesTablePanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  options: TraceTableOptions = {},
) {
  const panel = new table.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(tracesDatasource)
    .unit("short")
    .showHeader(true)
    .cellHeight("sm" as never);
  if (options.sortBy) {
    panel.withTransformation({
      id: "sortBy",
      options: {
        fields: {},
        sort: [
          {
            field: options.sortBy,
            desc: options.sortDesc ?? true,
          },
        ],
      },
    } as never);
  }
  if (options.organize) {
    panel.withTransformation({
      id: "organize",
      options: {
        excludeByName: options.organize.excludeByName ?? {},
        indexByName: options.organize.indexByName ?? {},
        renameByName: options.organize.renameByName ?? {},
      },
    } as never);
  }
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function logsPanel(
  id: number,
  title: string,
  grid: Grid,
  expr: string,
  _maxLines = 200,
) {
  return new logs.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(logsDatasource)
    .showLabels(true)
    .showTime(true)
    .showLogContextToggle(true)
    .wrapLogMessage(true)
    .prettifyLogMessage(false)
    .enableLogDetails(true)
    .sortOrder(common.LogsSortOrder.Descending)
    .dedupStrategy(common.LogsDedupStrategy.None)
    .fontSize("small")
    .detailsMode("sidebar")
    .withTarget(victoriaLogsInstant(expr));
}

export function barChartPanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  xField: string,
  unit = "short",
) {
  const panel = new barchart.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit)
    .xField(xField)
    .orientation(common.VizOrientation.Horizontal)
    .showValue(common.VisibilityMode.Always)
    .legend(
      new common.VizLegendOptionsBuilder()
        .showLegend(false)
        .displayMode(common.LegendDisplayMode.Hidden),
    )
    .withTransformation({
      id: "organize",
      options: {
        excludeByName: {
          Time: true,
          __name__: true,
          instance: true,
          job: true,
        },
        indexByName: {},
        renameByName: {
          Value: "value",
        },
      },
    } as never);
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function barGaugePanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  unit = "short",
) {
  const panel = new bargauge.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit);
  for (const target of targets) {
    const built = target.build() as cog.Dataquery & { instant?: boolean };
    panel.withTarget(raw({ ...built, instant: true }));
  }
  return panel;
}

export function piePanel(
  id: number,
  title: string,
  grid: Grid,
  targets: Builder<cog.Dataquery>[],
  unit = "short",
) {
  const panel = new piechart.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .datasource(datasource)
    .unit(unit)
    .pieType(piechart.PieChartType.Donut)
    .displayLabels([piechart.PieChartLabels.Name, piechart.PieChartLabels.Percent])
    .legend(
      new piechart.PieChartLegendOptionsBuilder()
        .showLegend(true)
        .displayMode(common.LegendDisplayMode.List)
        .placement(common.LegendPlacement.Bottom),
    );
  for (const target of targets) panel.withTarget(target);
  return panel;
}

export function textPanel(id: number, title: string, grid: Grid, content: string) {
  return new text.PanelBuilder()
    .id(id)
    .title(title)
    .gridPos(grid)
    .content(content)
    .mode("markdown" as never);
}

export function alertListPanel(id: number, title: string, grid: Grid) {
  return new dashboard.PanelBuilder()
    .id(id)
    .type("alertlist")
    .title(title)
    .gridPos(grid)
    .options({
      dashboardAlerts: false,
      maxItems: 20,
      showInstances: true,
      sortOrder: 1,
      stateFilter: { error: true, firing: true, noData: true, normal: false, pending: true },
      viewMode: "list",
    });
}

export function datasourceVariable() {
  return new dashboard.DatasourceVariableBuilder("datasource")
    .label("Datasource")
    .type("prometheus")
    .current({ selected: false, text: "Metrics", value: "prometheus" });
}

export function queryVariable(
  name: string,
  label: string,
  query: string,
  current = "manga-server",
  includeAll = false,
) {
  const builder = new dashboard.QueryVariableBuilder(name)
    .label(label)
    .datasource(datasource)
    .definition(query)
    .query({ query, refId: "StandardVariableQuery" })
    .refresh(2 as never)
    .sort(1 as never)
    .current({ selected: false, text: current, value: current });
  if (includeAll) {
    builder
      .includeAll(true)
      .multi(true)
      .allValue(".*")
      .current({ selected: true, text: "All", value: "$__all" });
  }
  return builder;
}

export function textVariable(name: string, label: string, value: string) {
  return new dashboard.TextBoxVariableBuilder(name)
    .label(label)
    .defaultValue(value)
    .current({ selected: false, text: value, value });
}

export function link(title: string, url: string, includeVars = false) {
  return new dashboard.DashboardLinkBuilder(title)
    .type("link" as never)
    .icon(title.includes("target") || title.includes("metrics") ? "external link" : "dashboard")
    .url(url)
    .includeVars(includeVars)
    .keepTime(includeVars)
    .targetBlank(url.startsWith("http"));
}

export function boundedRatio(numerator: string, denominator: string) {
  return `clamp_max((${numerator}) / clamp_min(${denominator}, 0.001), 1)`;
}

export function base(
  title: string,
  uid: string,
  tags: string[],
  timeFrom: string,
  refresh = "10s",
  version = 200,
) {
  return new dashboard.DashboardBuilder(title)
    .uid(uid)
    .tags(tags)
    .editable()
    .timezone("browser")
    .time({ from: timeFrom, to: "now" })
    .refresh(refresh)
    .version(version)
    .tooltip(1 as never)
    .liveNow(false)
    .weekStart("")
    .fiscalYearStartMonth(0)
    .withVariable(datasourceVariable());
}
