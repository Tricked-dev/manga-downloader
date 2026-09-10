use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use tempfile::{Builder, TempDir};

use crate::{
    Transformation, Workflow, WorkflowMode,
    config::Config,
    git::{CommitInfo, GitWorktree},
    tree,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanOptions {
    pub workflow: Option<String>,
    pub work_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncOptions {
    pub workflow: Option<String>,
    pub work_dir: Option<PathBuf>,
    pub push: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowPlan {
    pub workflow: String,
    pub mode: WorkflowMode,
    pub source_head: String,
    pub last_synced: Option<String>,
    pub commits: Vec<CommitInfo>,
    pub pull_request: Option<crate::PullRequestOptions>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncReport {
    pub workflow: String,
    pub pending: usize,
    pub committed: usize,
    pub empty_commits: usize,
}

pub fn plan_workflows(config: &Config, options: &PlanOptions) -> Result<Vec<WorkflowPlan>> {
    let mut plans = Vec::new();
    for workflow in select_workflows(config, options.workflow.as_deref())? {
        let execution = WorkflowExecution::prepare(workflow, options.work_dir.as_deref())?;
        plans.push(execution.into_plan());
    }
    Ok(plans)
}

pub fn sync_workflows(config: &Config, options: &SyncOptions) -> Result<Vec<SyncReport>> {
    let mut reports = Vec::new();
    for workflow in select_workflows(config, options.workflow.as_deref())? {
        let execution = WorkflowExecution::prepare(workflow, options.work_dir.as_deref())?;
        reports.push(execution.sync(options.push)?);
    }
    Ok(reports)
}

struct WorkflowExecution<'a> {
    workflow: &'a Workflow,
    prepared: PreparedWorkflow,
}

impl<'a> WorkflowExecution<'a> {
    fn prepare(workflow: &'a Workflow, work_dir: Option<&Path>) -> Result<Self> {
        Ok(Self {
            workflow,
            prepared: prepare_workflow(workflow, work_dir)?,
        })
    }

    fn into_plan(self) -> WorkflowPlan {
        self.prepared.plan
    }

    fn sync(&self, push: bool) -> Result<SyncReport> {
        let pending = self.prepared.plan.commits.len();
        let (committed, empty_commits) = match self.workflow.mode {
            WorkflowMode::Iterative => self.apply_iterative()?,
            WorkflowMode::Squash => self.apply_squash()?,
        };

        self.push_if_requested(push, committed)?;

        Ok(SyncReport {
            workflow: self.workflow.name.clone(),
            pending,
            committed,
            empty_commits,
        })
    }

    fn apply_iterative(&self) -> Result<(usize, usize)> {
        let mut committed = 0;
        let mut empty_commits = 0;
        for commit in &self.prepared.plan.commits {
            let is_empty = self.apply_source_commit(commit)?;
            if is_empty {
                empty_commits += 1;
            } else {
                committed += 1;
            }
        }
        Ok((committed, empty_commits))
    }

    fn apply_squash(&self) -> Result<(usize, usize)> {
        if self.prepared.plan.commits.is_empty() {
            return Ok((0, 0));
        }

        if self.apply_squash_commit()? {
            Ok((0, 1))
        } else {
            Ok((1, 0))
        }
    }

    fn push_if_requested(&self, push: bool, committed: usize) -> Result<()> {
        if !push || committed == 0 {
            return Ok(());
        }

        let push_reference = self
            .workflow
            .destination
            .push_reference
            .as_deref()
            .unwrap_or(&self.workflow.destination.reference);
        self.prepared
            .destination
            .push_head(push_reference)
            .with_context(|| format!("failed to push workflow {}", self.workflow.name))
    }

    fn apply_source_commit(&self, commit: &CommitInfo) -> Result<bool> {
        let tree_path = self
            .prepared
            .work_dir
            .path
            .join(format!("tree-{}", commit.short_sha()));
        self.export_transformed_tree(&commit.sha, &tree_path)?;

        let message = public_commit_message(self.workflow, commit);
        let committed = self
            .prepared
            .destination
            .commit_with_message(
                commit,
                &self.workflow.name,
                &message,
                self.workflow.authoring.as_ref(),
            )
            .with_context(|| format!("failed to commit synchronized source {}", commit.sha))?;
        Ok(!committed)
    }

    fn apply_squash_commit(&self) -> Result<bool> {
        let source_head = self.prepared.plan.source_head.clone();
        let tree_path = self
            .prepared
            .work_dir
            .path
            .join(format!("tree-squash-{}", short_sha(&source_head)));
        self.export_transformed_tree(&source_head, &tree_path)?;

        let source_head_commit = self.prepared.source.commit_info(&source_head)?;
        let message = squash_message(self.workflow, &self.prepared.plan.commits);
        let commit = CommitInfo {
            sha: source_head,
            subject: first_message_line(&message),
            author_name: source_head_commit.author_name,
            author_email: source_head_commit.author_email,
            message: message.clone(),
        };

        let committed = self
            .prepared
            .destination
            .commit_with_message(
                &commit,
                &self.workflow.name,
                &message,
                self.workflow.authoring.as_ref(),
            )
            .with_context(|| {
                format!(
                    "failed to commit squashed synchronized source {}",
                    self.prepared.plan.source_head
                )
            })?;
        Ok(!committed)
    }

    fn export_transformed_tree(&self, source_rev: &str, tree_path: &Path) -> Result<()> {
        tree::reset_dir(tree_path)?;
        self.prepared
            .source
            .export_tree(source_rev, tree_path)
            .with_context(|| format!("failed to export source commit {source_rev}"))?;
        if let Some(origin_files) = &self.workflow.origin_files {
            tree::keep_matching_files(tree_path, origin_files).with_context(|| {
                format!("failed to filter source files for commit {source_rev}")
            })?;
        }
        tree::apply_transformations(tree_path, &self.workflow.transformations)
            .with_context(|| format!("failed to transform source commit {source_rev}"))?;
        tree::replace_destination(
            tree_path,
            self.prepared.destination.path(),
            &self.workflow.destination_files,
        )
        .with_context(|| {
            format!("failed to update destination files for source commit {source_rev}")
        })?;
        Ok(())
    }
}

struct PreparedWorkflow {
    work_dir: WorkDir,
    source: GitWorktree,
    destination: GitWorktree,
    plan: WorkflowPlan,
}

fn prepare_workflow(workflow: &Workflow, work_dir: Option<&Path>) -> Result<PreparedWorkflow> {
    let work_dir = WorkDir::create(workflow, work_dir)?;
    let source_path = work_dir.path.join("source");
    let destination_path = work_dir.path.join("destination");

    let source = GitWorktree::clone_for_source(&workflow.origin, &source_path)
        .with_context(|| format!("failed to prepare source for workflow {}", workflow.name))?;
    let destination = GitWorktree::clone_for_destination(&workflow.destination, &destination_path)
        .with_context(|| {
            format!(
                "failed to prepare destination for workflow {}",
                workflow.name
            )
        })?;

    let source_head = source.source_head()?;
    let last_synced = destination.find_last_synced_rev(&workflow.name)?;
    let commits = pending_commits(workflow, &source, last_synced.as_deref(), &source_head)?;

    Ok(PreparedWorkflow {
        work_dir,
        source,
        destination,
        plan: WorkflowPlan {
            workflow: workflow.name.clone(),
            mode: workflow.mode.clone(),
            source_head,
            last_synced,
            commits,
            pull_request: workflow.destination.pull_request.clone(),
        },
    })
}

fn pending_commits(
    workflow: &Workflow,
    source: &GitWorktree,
    last_synced: Option<&str>,
    source_head: &str,
) -> Result<Vec<CommitInfo>> {
    let shas = if let Some(last_synced) = last_synced {
        if last_synced == source_head {
            Vec::new()
        } else {
            if !source.is_ancestor(last_synced, source_head)? {
                bail!(
                    "workflow {} last synced source rev {} is not an ancestor of {}",
                    workflow.name,
                    last_synced,
                    source_head
                );
            }
            source.rev_list_reverse(last_synced, source_head)?
        }
    } else if let Some(first_commit) = &workflow.first_commit {
        let all = source.rev_list_all_reverse(source_head)?;
        let start = all
            .iter()
            .position(|sha| sha == first_commit)
            .with_context(|| {
                format!(
                    "workflow {} first_commit {} was not found in source history",
                    workflow.name, first_commit
                )
            })?;
        all[start..].to_vec()
    } else {
        vec![source_head.to_owned()]
    };

    shas.into_iter()
        .map(|sha| source.commit_info(&sha))
        .collect()
}

fn select_workflows<'a>(config: &'a Config, selected: Option<&str>) -> Result<Vec<&'a Workflow>> {
    match selected {
        Some(name) => {
            let workflow = config
                .workflows
                .iter()
                .find(|workflow| workflow.name == name)
                .with_context(|| format!("unknown workflow {name}"))?;
            Ok(vec![workflow])
        }
        None => Ok(config.workflows.iter().collect()),
    }
}

#[derive(Clone, Debug)]
struct SquashNotes {
    prefix: String,
    max: usize,
    compact: bool,
    show_ref: bool,
    show_author: bool,
    show_description: bool,
    oldest_first: bool,
}

impl Default for SquashNotes {
    fn default() -> Self {
        Self {
            prefix: "Copybara import of the project:\n\n".to_owned(),
            max: 100,
            compact: true,
            show_ref: true,
            show_author: true,
            show_description: true,
            oldest_first: false,
        }
    }
}

#[derive(Clone, Debug)]
struct PublicCommitMessages {
    begin_marker: String,
    end_marker: String,
    fallback: String,
}

fn squash_message(workflow: &Workflow, commits: &[CommitInfo]) -> String {
    let notes = squash_notes(workflow);
    let public_messages = public_commit_messages(workflow);
    let mut message = notes.prefix;
    if !message.ends_with('\n') {
        message.push('\n');
    }

    let mut ordered = commits.iter().collect::<Vec<_>>();
    if !notes.oldest_first {
        ordered.reverse();
    }

    let emitted = ordered.len().min(notes.max);
    for commit in ordered.into_iter().take(emitted) {
        message.push_str("- ");
        if notes.show_ref {
            message.push_str(commit.short_sha());
        }
        if notes.show_description {
            append_with_space(
                &mut message,
                message_summary(commit, notes.compact, public_messages.as_ref()).trim(),
            );
        }
        if notes.show_author {
            append_with_space(&mut message, &format!("by {}", commit.author_name));
        }
        message.push('\n');
    }

    let omitted = commits.len().saturating_sub(emitted);
    if omitted > 0 {
        message.push_str(&format!("- and {omitted} more\n"));
    }
    if commits.is_empty() {
        message.push_str("No source changes.\n");
    }
    message
}

fn squash_notes(workflow: &Workflow) -> SquashNotes {
    workflow
        .transformations
        .iter()
        .find_map(|transformation| match transformation {
            Transformation::MetadataSquashNotes {
                prefix,
                max,
                compact,
                show_ref,
                show_author,
                show_description,
                oldest_first,
            } => Some(SquashNotes {
                prefix: prefix.clone(),
                max: *max,
                compact: *compact,
                show_ref: *show_ref,
                show_author: *show_author,
                show_description: *show_description,
                oldest_first: *oldest_first,
            }),
            _ => None,
        })
        .unwrap_or_default()
}

fn public_commit_message(workflow: &Workflow, commit: &CommitInfo) -> String {
    if let Some(settings) = public_commit_messages(workflow) {
        extract_public_message(commit, &settings)
    } else {
        commit
            .message
            .trim_end()
            .to_owned()
            .if_empty_then(|| format!("Sync {}", commit.short_sha()))
    }
}

fn public_commit_messages(workflow: &Workflow) -> Option<PublicCommitMessages> {
    workflow
        .transformations
        .iter()
        .find_map(|transformation| match transformation {
            Transformation::PublicCommitMessages {
                begin_marker,
                end_marker,
                fallback,
            } => Some(PublicCommitMessages {
                begin_marker: begin_marker.clone(),
                end_marker: end_marker.clone(),
                fallback: fallback.clone(),
            }),
            _ => None,
        })
}

fn extract_public_message(commit: &CommitInfo, settings: &PublicCommitMessages) -> String {
    let Some(after_begin) = commit.message.split_once(&settings.begin_marker) else {
        return settings.fallback.clone();
    };
    let Some((public, _)) = after_begin.1.split_once(&settings.end_marker) else {
        return settings.fallback.clone();
    };
    let public = public.trim();
    if public.is_empty() {
        settings.fallback.clone()
    } else {
        public.to_owned()
    }
}

fn append_with_space(message: &mut String, value: &str) {
    if value.is_empty() {
        return;
    }
    if !message.ends_with(' ') && !message.ends_with("- ") {
        message.push(' ');
    }
    message.push_str(value);
}

fn message_summary(
    commit: &CommitInfo,
    compact: bool,
    public_messages: Option<&PublicCommitMessages>,
) -> String {
    if let Some(settings) = public_messages {
        return extract_public_message(commit, settings);
    }

    if compact {
        commit.subject.clone()
    } else {
        commit.message.trim().to_owned()
    }
}

fn first_message_line(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("sync source changes")
        .to_owned()
}

fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

trait EmptyStringFallback {
    fn if_empty_then(self, fallback: impl FnOnce() -> String) -> String;
}

impl EmptyStringFallback for String {
    fn if_empty_then(self, fallback: impl FnOnce() -> String) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}

struct WorkDir {
    path: PathBuf,
    _temp_dir: Option<TempDir>,
}

impl WorkDir {
    fn create(workflow: &Workflow, base: Option<&Path>) -> Result<Self> {
        match base {
            Some(base) => {
                let path = base.join(&workflow.name);
                if path.exists() {
                    fs::remove_dir_all(&path)
                        .with_context(|| format!("failed to remove {}", path.display()))?;
                }
                fs::create_dir_all(&path)
                    .with_context(|| format!("failed to create {}", path.display()))?;
                Ok(Self {
                    path,
                    _temp_dir: None,
                })
            }
            None => {
                let temp_dir = Builder::new()
                    .prefix("codesync-")
                    .tempdir()
                    .context("failed to create temporary codesync work directory")?;
                Ok(Self {
                    path: temp_dir.path().to_path_buf(),
                    _temp_dir: Some(temp_dir),
                })
            }
        }
    }
}
