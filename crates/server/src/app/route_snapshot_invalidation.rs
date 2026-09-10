use crate::{AppState, api::response_cache};

const DOWNLOAD_LIST_ROUTE_SNAPSHOT_NAMES: &[&str] = &["downloads:list", "stats:overview"];

const DOWNLOADED_ARCHIVE_CREATED_ROUTE_SNAPSHOT_NAMES: &[&str] = &[
    "downloads:list",
    "library:all-chapters",
    "library:list",
    "library:updates",
    "stats:overview",
];

const DOWNLOADED_ARCHIVE_DELETED_ROUTE_SNAPSHOT_NAMES: &[&str] = &[
    "downloads:list",
    "library:all-chapters",
    "library:list",
    "stats:overview",
];

const DOWNLOADED_ARCHIVE_METADATA_CHANGED_ROUTE_SNAPSHOT_NAMES: &[&str] = &["stats:overview"];

const SOURCE_CATALOG_ROUTE_SNAPSHOT_NAMES: &[&str] =
    &["library:list", "sources:list", "stats:overview"];

const LOCAL_LIBRARY_ROUTE_SNAPSHOT_NAMES: &[&str] = &[
    "downloads:list",
    "library:all-chapters",
    "library:list",
    "library:updates",
    "stats:overview",
];

const READ_PROGRESS_ROUTE_SNAPSHOT_NAMES: &[&str] = &["library:all-chapters", "stats:overview"];

const SETTINGS_ROUTE_SNAPSHOT_NAMES: &[&str] = &["settings:get", "stats:overview"];

const DATABASE_MAINTENANCE_ROUTE_SNAPSHOT_NAMES: &[&str] = &[];

const STATS_MAINTENANCE_ROUTE_SNAPSHOT_NAMES: &[&str] = &["stats:overview"];

pub(crate) fn download_list_changed(state: &AppState) {
    invalidate_route_snapshots(state, download_list_route_snapshot_names());
}

pub(crate) fn downloaded_archive_created(state: &AppState, _download_id: &str, _chapter_id: &str) {
    invalidate_route_snapshots(state, downloaded_archive_created_route_snapshot_names());
}

pub(crate) fn downloaded_archive_deleted(state: &AppState, _download_id: &str, _chapter_id: &str) {
    invalidate_route_snapshots(state, downloaded_archive_deleted_route_snapshot_names());
}

pub(crate) fn downloaded_archive_metadata_changed(
    state: &AppState,
    _download_id: &str,
    _chapter_id: &str,
) {
    invalidate_route_snapshots(
        state,
        downloaded_archive_metadata_changed_route_snapshot_names(),
    );
}

pub(crate) fn source_enabled_changed(state: &AppState, _source: &str, _enabled: bool) {
    invalidate_route_snapshots(state, source_catalog_route_snapshot_names());
}

pub(crate) fn source_settings_changed(state: &AppState, _source: &str) {
    invalidate_route_snapshots(state, source_catalog_route_snapshot_names());
}

pub(crate) fn local_library_changed(state: &AppState) {
    invalidate_route_snapshots(state, local_library_route_snapshot_names());
}

pub(crate) fn read_progress_changed(state: &AppState, _chapter_id: &str) {
    invalidate_route_snapshots(state, read_progress_route_snapshot_names());
}

pub(crate) fn settings_changed(state: &AppState) {
    invalidate_route_snapshots(state, settings_route_snapshot_names());
}

pub(crate) fn database_maintenance_completed(state: &AppState) {
    invalidate_route_snapshots(state, database_maintenance_route_snapshot_names());
}

#[allow(dead_code)]
pub(crate) fn stats_maintenance_completed(state: &AppState) {
    invalidate_route_snapshots(state, stats_maintenance_route_snapshot_names());
}

fn invalidate_route_snapshots(state: &AppState, names: &[&str]) {
    for name in names {
        state
            .cache
            .remove(&response_cache::route_snapshot_key(name));
    }
}

const fn download_list_route_snapshot_names() -> &'static [&'static str] {
    DOWNLOAD_LIST_ROUTE_SNAPSHOT_NAMES
}

const fn downloaded_archive_created_route_snapshot_names() -> &'static [&'static str] {
    DOWNLOADED_ARCHIVE_CREATED_ROUTE_SNAPSHOT_NAMES
}

const fn downloaded_archive_deleted_route_snapshot_names() -> &'static [&'static str] {
    DOWNLOADED_ARCHIVE_DELETED_ROUTE_SNAPSHOT_NAMES
}

const fn downloaded_archive_metadata_changed_route_snapshot_names() -> &'static [&'static str] {
    DOWNLOADED_ARCHIVE_METADATA_CHANGED_ROUTE_SNAPSHOT_NAMES
}

const fn source_catalog_route_snapshot_names() -> &'static [&'static str] {
    SOURCE_CATALOG_ROUTE_SNAPSHOT_NAMES
}

const fn local_library_route_snapshot_names() -> &'static [&'static str] {
    LOCAL_LIBRARY_ROUTE_SNAPSHOT_NAMES
}

const fn read_progress_route_snapshot_names() -> &'static [&'static str] {
    READ_PROGRESS_ROUTE_SNAPSHOT_NAMES
}

const fn settings_route_snapshot_names() -> &'static [&'static str] {
    SETTINGS_ROUTE_SNAPSHOT_NAMES
}

const fn database_maintenance_route_snapshot_names() -> &'static [&'static str] {
    DATABASE_MAINTENANCE_ROUTE_SNAPSHOT_NAMES
}

#[allow(dead_code)]
const fn stats_maintenance_route_snapshot_names() -> &'static [&'static str] {
    STATS_MAINTENANCE_ROUTE_SNAPSHOT_NAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_changes_map_to_route_snapshot_sets() {
        let mappings: &[(&str, &[&str], &[&str])] = &[
            (
                "download_list_changed",
                download_list_route_snapshot_names(),
                &["downloads:list", "stats:overview"],
            ),
            (
                "downloaded_archive_created",
                downloaded_archive_created_route_snapshot_names(),
                &[
                    "downloads:list",
                    "library:all-chapters",
                    "library:list",
                    "library:updates",
                    "stats:overview",
                ],
            ),
            (
                "downloaded_archive_deleted",
                downloaded_archive_deleted_route_snapshot_names(),
                &[
                    "downloads:list",
                    "library:all-chapters",
                    "library:list",
                    "stats:overview",
                ],
            ),
            (
                "downloaded_archive_metadata_changed",
                downloaded_archive_metadata_changed_route_snapshot_names(),
                &["stats:overview"],
            ),
            (
                "source_enabled_changed",
                source_catalog_route_snapshot_names(),
                &["library:list", "sources:list", "stats:overview"],
            ),
            (
                "source_settings_changed",
                source_catalog_route_snapshot_names(),
                &["library:list", "sources:list", "stats:overview"],
            ),
            (
                "local_library_changed",
                local_library_route_snapshot_names(),
                &[
                    "downloads:list",
                    "library:all-chapters",
                    "library:list",
                    "library:updates",
                    "stats:overview",
                ],
            ),
            (
                "read_progress_changed",
                read_progress_route_snapshot_names(),
                &["library:all-chapters", "stats:overview"],
            ),
            (
                "settings_changed",
                settings_route_snapshot_names(),
                &["settings:get", "stats:overview"],
            ),
            (
                "database_maintenance_completed",
                database_maintenance_route_snapshot_names(),
                &[],
            ),
            (
                "stats_maintenance_completed",
                stats_maintenance_route_snapshot_names(),
                &["stats:overview"],
            ),
        ];

        for (change, route_snapshot_names, expected) in mappings {
            assert_eq!(
                *route_snapshot_names, *expected,
                "{change} should invalidate the expected route snapshots",
            );
        }
    }
}
