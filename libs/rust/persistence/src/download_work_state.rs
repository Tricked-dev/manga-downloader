#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadWorkStatus {
    Queued,
    CancelRequested,
    FetchingPages,
    DownloadingAssets,
    TransformingAssets,
    Archiving,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadWorkStatus {
    pub const ACTIVE_PERSISTED_VALUES: [&'static str; 6] = [
        Self::Queued.persisted_value(),
        Self::CancelRequested.persisted_value(),
        Self::FetchingPages.persisted_value(),
        Self::DownloadingAssets.persisted_value(),
        Self::TransformingAssets.persisted_value(),
        Self::Archiving.persisted_value(),
    ];

    pub const EXTERNAL_LABELS: [&'static str; 8] = [
        "queued",
        "fetch",
        "conversion",
        "archive",
        "completed",
        "error",
        "canceling",
        "cancelled",
    ];

    pub const INTERRUPTED_PERSISTED_VALUES: [&'static str; 5] = [
        Self::CancelRequested.persisted_value(),
        Self::FetchingPages.persisted_value(),
        Self::DownloadingAssets.persisted_value(),
        Self::TransformingAssets.persisted_value(),
        Self::Archiving.persisted_value(),
    ];

    #[must_use]
    /// Returns the database value for this status.
    pub const fn persisted_value(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CancelRequested => "cancel_requested",
            Self::FetchingPages => "fetching_pages",
            Self::DownloadingAssets => "downloading_assets",
            Self::TransformingAssets => "transforming_assets",
            Self::Archiving => "archiving",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    #[must_use]
    /// Returns the public status label exposed to clients and metrics.
    pub const fn external_label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CancelRequested => "canceling",
            Self::FetchingPages | Self::DownloadingAssets => "fetch",
            Self::TransformingAssets => "conversion",
            Self::Archiving => "archive",
            Self::Completed => "completed",
            Self::Failed => "error",
            Self::Cancelled => "cancelled",
        }
    }

    #[must_use]
    /// Parses a persisted database status value.
    pub fn from_persisted_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "cancel_requested" => Some(Self::CancelRequested),
            "fetching_pages" => Some(Self::FetchingPages),
            "downloading_assets" => Some(Self::DownloadingAssets),
            "transforming_assets" => Some(Self::TransformingAssets),
            "archiving" => Some(Self::Archiving),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    #[must_use]
    /// Returns whether a public status label represents in-progress work.
    pub const fn external_label_is_active(label: &str) -> bool {
        matches!(
            label.as_bytes(),
            b"queued" | b"canceling" | b"fetch" | b"conversion" | b"archive"
        )
    }

    #[must_use]
    /// Returns whether a public status label can be actively cancelled.
    pub const fn external_label_requires_cancellation(label: &str) -> bool {
        matches!(
            label.as_bytes(),
            b"canceling" | b"fetch" | b"conversion" | b"archive"
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadWorkTransition {
    Queued,
    CancelRequested,
    FetchingPages,
    TransformingAssets,
    Archiving,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadWorkTransition {
    #[must_use]
    /// Returns the persisted status produced by this transition.
    pub const fn persisted_status(self) -> DownloadWorkStatus {
        match self {
            Self::Queued => DownloadWorkStatus::Queued,
            Self::CancelRequested => DownloadWorkStatus::CancelRequested,
            Self::FetchingPages => DownloadWorkStatus::FetchingPages,
            Self::TransformingAssets => DownloadWorkStatus::TransformingAssets,
            Self::Archiving => DownloadWorkStatus::Archiving,
            Self::Completed => DownloadWorkStatus::Completed,
            Self::Failed => DownloadWorkStatus::Failed,
            Self::Cancelled => DownloadWorkStatus::Cancelled,
        }
    }

    #[must_use]
    /// Returns the event-stage label recorded for this transition.
    pub const fn event_stage(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CancelRequested => "canceling",
            Self::FetchingPages => "fetch",
            Self::TransformingAssets => "conversion",
            Self::Archiving => "archive",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    #[must_use]
    /// Returns the default error code associated with this transition.
    pub const fn default_error_code(self) -> Option<&'static str> {
        match self {
            Self::Failed => Some("download_failed"),
            Self::Queued
            | Self::CancelRequested
            | Self::FetchingPages
            | Self::TransformingAssets
            | Self::Archiving
            | Self::Completed
            | Self::Cancelled => None,
        }
    }

    #[must_use]
    /// Returns whether this transition ends a download attempt.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_statuses_map_to_public_download_labels() {
        let mappings = [
            (DownloadWorkStatus::Queued, "queued"),
            (DownloadWorkStatus::CancelRequested, "canceling"),
            (DownloadWorkStatus::FetchingPages, "fetch"),
            (DownloadWorkStatus::DownloadingAssets, "fetch"),
            (DownloadWorkStatus::TransformingAssets, "conversion"),
            (DownloadWorkStatus::Archiving, "archive"),
            (DownloadWorkStatus::Completed, "completed"),
            (DownloadWorkStatus::Failed, "error"),
            (DownloadWorkStatus::Cancelled, "cancelled"),
        ];

        for (status, label) in mappings {
            assert_eq!(status.external_label(), label);
            assert_eq!(
                DownloadWorkStatus::from_persisted_value(status.persisted_value()),
                Some(status),
            );
        }
    }

    #[test]
    fn active_public_download_labels_cover_work_in_progress() {
        for label in ["queued", "canceling", "fetch", "conversion", "archive"] {
            assert!(DownloadWorkStatus::external_label_is_active(label));
        }

        for label in ["completed", "error", "cancelled"] {
            assert!(!DownloadWorkStatus::external_label_is_active(label));
        }
    }

    #[test]
    fn queued_downloads_do_not_need_cancellation() {
        assert!(!DownloadWorkStatus::external_label_requires_cancellation(
            "queued"
        ));

        for label in ["canceling", "fetch", "conversion", "archive"] {
            assert!(DownloadWorkStatus::external_label_requires_cancellation(
                label
            ));
        }
    }
}
