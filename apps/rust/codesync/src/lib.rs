#![warn(clippy::correctness, clippy::suspicious, clippy::style, clippy::perf)]

pub mod cli;
pub mod config;
mod config_authoring;
mod git;
mod glob;
mod sync;
mod tree;

pub use config::{
    Authoring, AuthoringMode, FileSet, GitRepository, PullRequestOptions, Transformation, Workflow,
    WorkflowMode, load_config, load_config_from_str,
};
pub use sync::{
    PlanOptions, SyncOptions, SyncReport, WorkflowPlan, plan_workflows, sync_workflows,
};
