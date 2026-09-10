use anyhow::Result;
use backend_persistence::{Database, DownloadWorkStatus, DownloadWorkTransition};

const FETCH_PHASE_END: f64 = 60.0;
const FETCH_PROGRESS_PERSIST_STEP: f64 = 5.0;
const CONVERSION_PHASE_END: f64 = 95.0;

pub(crate) fn external_status_requires_cancellation(status: &str) -> bool {
    DownloadWorkStatus::external_label_requires_cancellation(status)
}

pub(crate) fn external_status_is_completed(status: &str) -> bool {
    status == DownloadWorkStatus::Completed.external_label()
}

pub(crate) fn external_status_is_failed(status: &str) -> bool {
    status == DownloadWorkStatus::Failed.external_label()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DownloadWorkPhase {
    transition: DownloadWorkTransition,
    progress: f64,
}

impl DownloadWorkPhase {
    const fn fetching_pages() -> Self {
        Self {
            transition: DownloadWorkTransition::FetchingPages,
            progress: 0.0,
        }
    }

    const fn transforming_assets_completed() -> Self {
        Self {
            transition: DownloadWorkTransition::TransformingAssets,
            progress: CONVERSION_PHASE_END,
        }
    }

    const fn archiving() -> Self {
        Self {
            transition: DownloadWorkTransition::Archiving,
            progress: CONVERSION_PHASE_END,
        }
    }

    const fn cancel_requested(current_progress: f64) -> Self {
        Self {
            transition: DownloadWorkTransition::CancelRequested,
            progress: current_progress,
        }
    }

    const fn cancelled() -> Self {
        Self {
            transition: DownloadWorkTransition::Cancelled,
            progress: 0.0,
        }
    }

    const fn failed() -> Self {
        Self {
            transition: DownloadWorkTransition::Failed,
            progress: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DownloadWorkStateProgression<'a> {
    db: &'a Database,
    download_id: &'a str,
}

impl<'a> DownloadWorkStateProgression<'a> {
    pub(crate) const fn new(db: &'a Database, download_id: &'a str) -> Self {
        Self { db, download_id }
    }

    pub(crate) async fn fetching_pages(&self) -> Result<()> {
        self.enter_phase(DownloadWorkPhase::fetching_pages()).await
    }

    pub(crate) async fn transforming_assets_completed(&self) -> Result<()> {
        self.enter_phase(DownloadWorkPhase::transforming_assets_completed())
            .await
    }

    pub(crate) async fn archiving(&self) -> Result<()> {
        self.enter_phase(DownloadWorkPhase::archiving()).await
    }

    pub(crate) async fn cancel_requested(&self, current_progress: f64) -> Result<()> {
        self.enter_phase(DownloadWorkPhase::cancel_requested(current_progress))
            .await
    }

    pub(crate) async fn cancelled(&self) -> Result<()> {
        self.enter_phase(DownloadWorkPhase::cancelled()).await
    }

    pub(crate) async fn failed(&self, error: &str) -> Result<()> {
        self.transition_to_phase(DownloadWorkPhase::failed(), Some(error))
            .await
    }

    pub(crate) fn fetch_progress(&self, total_pages: usize) -> DownloadFetchProgress<'a> {
        DownloadFetchProgress {
            progression: *self,
            policy: FetchProgressPolicy::new(total_pages),
        }
    }

    async fn enter_phase(&self, phase: DownloadWorkPhase) -> Result<()> {
        self.transition_to_phase(phase, None).await
    }

    async fn transition_to_phase(
        &self,
        phase: DownloadWorkPhase,
        error: Option<&str>,
    ) -> Result<()> {
        self.db
            .update_download_status(self.download_id, phase.transition, phase.progress, error)
            .await?;
        Ok(())
    }
}

pub(crate) struct DownloadFetchProgress<'a> {
    progression: DownloadWorkStateProgression<'a>,
    policy: FetchProgressPolicy,
}

impl DownloadFetchProgress<'_> {
    pub(crate) async fn refresh(&mut self, completed_pages: usize) -> Result<()> {
        let progress = self.policy.progress_percent(completed_pages);
        if self.policy.should_persist(progress, completed_pages) {
            self.progression
                .db
                .update_download_progress(self.progression.download_id, progress)
                .await?;
            self.policy.mark_persisted(progress);
        }
        Ok(())
    }
}

struct FetchProgressPolicy {
    total_pages: usize,
    last_persisted_progress: f64,
}

impl FetchProgressPolicy {
    const fn new(total_pages: usize) -> Self {
        Self {
            total_pages,
            last_persisted_progress: 0.0,
        }
    }

    fn progress_percent(&self, completed_pages: usize) -> f64 {
        if self.total_pages == 0 {
            return 0.0;
        }

        let completed_pages = completed_pages.min(self.total_pages);
        let completed_pages = f64::from(u32::try_from(completed_pages).unwrap_or(u32::MAX));
        let total_pages = f64::from(u32::try_from(self.total_pages).unwrap_or(u32::MAX));
        (completed_pages / total_pages) * FETCH_PHASE_END
    }

    fn should_persist(&self, progress: f64, completed_pages: usize) -> bool {
        completed_pages == self.total_pages
            || progress >= self.last_persisted_progress + FETCH_PROGRESS_PERSIST_STEP
    }

    fn mark_persisted(&mut self, progress: f64) {
        self.last_persisted_progress = progress;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_work_phases_define_transition_progress_boundaries() {
        assert_eq!(
            DownloadWorkPhase::fetching_pages(),
            DownloadWorkPhase {
                transition: DownloadWorkTransition::FetchingPages,
                progress: 0.0,
            }
        );
        assert_eq!(
            DownloadWorkPhase::transforming_assets_completed(),
            DownloadWorkPhase {
                transition: DownloadWorkTransition::TransformingAssets,
                progress: CONVERSION_PHASE_END,
            }
        );
        assert_eq!(
            DownloadWorkPhase::archiving(),
            DownloadWorkPhase {
                transition: DownloadWorkTransition::Archiving,
                progress: CONVERSION_PHASE_END,
            }
        );
    }

    #[test]
    fn terminal_download_work_phases_reset_progress() {
        assert_eq!(DownloadWorkPhase::cancelled().progress, 0.0);
        assert_eq!(DownloadWorkPhase::failed().progress, 0.0);
    }

    #[test]
    fn fetch_progress_spans_zero_to_fetch_completion() {
        let policy = FetchProgressPolicy::new(12);

        assert_eq!(policy.progress_percent(0), 0.0);
        assert_eq!(policy.progress_percent(6), 30.0);
        assert_eq!(policy.progress_percent(12), FETCH_PHASE_END);
        assert_eq!(policy.progress_percent(14), FETCH_PHASE_END);
    }

    #[test]
    fn fetch_progress_persists_by_step_or_completion() {
        let mut policy = FetchProgressPolicy::new(100);

        assert!(!policy.should_persist(4.9, 8));
        assert!(policy.should_persist(5.0, 9));
        policy.mark_persisted(5.0);

        assert!(!policy.should_persist(9.9, 16));
        assert!(policy.should_persist(10.0, 17));
        assert!(policy.should_persist(10.0, 100));
    }
}
