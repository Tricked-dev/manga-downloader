use std::{collections::BTreeSet, fs, path::Path};

use allocative::Allocative;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Allocative)]
pub struct Config {
    pub workflows: Vec<Workflow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Workflow {
    pub name: String,
    pub origin: GitRepository,
    pub destination: GitRepository,
    pub destination_files: FileSet,
    pub transformations: Vec<Transformation>,
    pub first_commit: Option<String>,
    pub origin_files: Option<FileSet>,
    pub mode: WorkflowMode,
    pub authoring: Option<Authoring>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct GitRepository {
    pub url: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub push_reference: Option<String>,
    pub pull_request: Option<PullRequestOptions>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct PullRequestOptions {
    pub branch: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub update_description: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Authoring {
    #[serde(default)]
    pub mode: AuthoringMode,
    pub default_author: Option<String>,
    #[serde(default)]
    pub allowlist: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Allocative)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringMode {
    #[default]
    PassThru,
    Overwrite,
    Allowed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Allocative)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowMode {
    Squash,
    #[default]
    Iterative,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct FileSet {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl FileSet {
    #[must_use]
    pub fn all() -> Self {
        Self {
            include: vec!["**".to_owned()],
            exclude: Vec::new(),
        }
    }

    #[must_use]
    pub fn matches(&self, path: &Path) -> bool {
        crate::glob::matches_file_set(self, path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Allocative)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transformation {
    Copy {
        from: String,
        to: String,
        paths: FileSet,
    },
    Move {
        from: String,
        to: String,
    },
    Rename {
        before: String,
        after: String,
    },
    Remove {
        paths: FileSet,
    },
    Strip {
        paths: FileSet,
    },
    Replace {
        before: String,
        after: String,
        paths: FileSet,
    },
    MetadataSquashNotes {
        prefix: String,
        max: usize,
        compact: bool,
        show_ref: bool,
        show_author: bool,
        show_description: bool,
        oldest_first: bool,
    },
    PublicCommitMessages {
        begin_marker: String,
        end_marker: String,
        fallback: String,
    },
}

#[must_use]
pub fn starlark_globals() -> starlark::environment::Globals {
    crate::config_authoring::starlark_globals()
}

pub fn load_config(path: &Path) -> Result<Config> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    load_config_from_str(&path.display().to_string(), &source)
}

pub fn load_config_from_str(name: &str, source: &str) -> Result<Config> {
    crate::config_authoring::load_config_from_str(name, source)
}

pub(crate) fn validate_workflows(workflows: &[Workflow]) -> Result<()> {
    if workflows.is_empty() {
        bail!("config did not declare any workflow(...) entries");
    }

    let mut names = BTreeSet::new();
    for workflow in workflows {
        if workflow.name.trim().is_empty() {
            bail!("workflow name must not be empty");
        }
        if !names.insert(workflow.name.clone()) {
            bail!("duplicate workflow name {}", workflow.name);
        }
        if workflow.origin.url.trim().is_empty() {
            bail!("workflow {} origin url must not be empty", workflow.name);
        }
        if workflow.destination.url.trim().is_empty() {
            bail!(
                "workflow {} destination url must not be empty",
                workflow.name
            );
        }
        if workflow.destination_files.include.is_empty() {
            bail!(
                "workflow {} destination_files must include at least one pattern",
                workflow.name
            );
        }
    }

    Ok(())
}
