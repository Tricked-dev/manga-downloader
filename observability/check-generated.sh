#!/usr/bin/env bash
set -euo pipefail

root="${TEST_SRCDIR:?}/_main/observability"
generated="$root/generated"

required_files=(
  "$generated/docker-compose.yml"
  "$generated/grafana/dashboards/autometrics-function-explorer.json"
  "$generated/grafana/dashboards/autometrics-overview.json"
  "$generated/grafana/dashboards/manga-server-overview.json"
  "$generated/grafana/dashboards/manga-server-signals.json"
  "$generated/grafana/dashboards/tokio-runtime.json"
  "$generated/grafana/provisioning/dashboards/manga.yaml"
  "$generated/grafana/provisioning/datasources/datasources.yaml"
  "$generated/otel-collector/config.yaml"
  "$generated/tempo/tempo.yaml"
)

for file in "${required_files[@]}"; do
  test -s "$file"
done
