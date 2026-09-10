#![allow(clippy::too_many_arguments)]

use std::{cell::RefCell, fmt};

use allocative::Allocative;
use anyhow::{Context, Result, anyhow, bail};
use starlark::{
    any::ProvidesStaticType,
    environment::{GlobalsBuilder, LibraryExtension, Module},
    eval::Evaluator,
    starlark_module, starlark_simple_value,
    syntax::{AstModule, Dialect, DialectTypes},
    values::{
        NoSerialize, StarlarkPagablePanic, StarlarkValue, Value, list_or_tuple::UnpackListOrTuple,
        none::NoneType, starlark_value,
    },
};

use crate::config::{
    Authoring, AuthoringMode, Config, FileSet, GitRepository, PullRequestOptions, Transformation,
    Workflow, WorkflowMode, validate_workflows,
};

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, StarlarkPagablePanic)]
struct StarlarkGitRepository(GitRepository);

starlark_simple_value!(StarlarkGitRepository);

impl fmt::Display for StarlarkGitRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitRepository(url = {:?})", self.0.url)
    }
}

#[starlark_value(type = "GitRepository")]
impl<'v> StarlarkValue<'v> for StarlarkGitRepository {}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, StarlarkPagablePanic)]
struct StarlarkFileSet(FileSet);

starlark_simple_value!(StarlarkFileSet);

impl fmt::Display for StarlarkFileSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileSet(include = {:?})", self.0.include)
    }
}

#[starlark_value(type = "FileSet")]
impl<'v> StarlarkValue<'v> for StarlarkFileSet {}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, StarlarkPagablePanic)]
struct StarlarkTransformation(Transformation);

starlark_simple_value!(StarlarkTransformation);

impl fmt::Display for StarlarkTransformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transformation({:?})", self.0)
    }
}

#[starlark_value(type = "Transformation")]
impl<'v> StarlarkValue<'v> for StarlarkTransformation {}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, StarlarkPagablePanic)]
struct StarlarkAuthoring(Authoring);

starlark_simple_value!(StarlarkAuthoring);

impl fmt::Display for StarlarkAuthoring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Authoring({:?})", self.0.mode)
    }
}

#[starlark_value(type = "Authoring")]
impl<'v> StarlarkValue<'v> for StarlarkAuthoring {}

#[derive(Debug, ProvidesStaticType, Default)]
struct ConfigStore(RefCell<Vec<Workflow>>);

impl ConfigStore {
    fn push(&self, workflow: Workflow) {
        self.0.borrow_mut().push(workflow);
    }

    fn into_config(self) -> Result<Config> {
        let workflows = self.0.into_inner();
        validate_workflows(&workflows)?;
        Ok(Config { workflows })
    }
}

#[must_use]
pub(crate) fn starlark_globals() -> starlark::environment::Globals {
    let mut builder = GlobalsBuilder::extended_by(&[
        LibraryExtension::Typing,
        LibraryExtension::RecordType,
        LibraryExtension::EnumType,
    ])
    .with(codesync_types_api)
    .with(codesync_config_api);
    builder.namespace("core", codesync_core_api);
    builder.namespace("git", codesync_git_api);
    builder.namespace("authoring", codesync_authoring_api);
    builder.namespace("metadata", codesync_metadata_api);
    builder.namespace("service", codesync_service_api);
    builder.build()
}

pub(crate) fn load_config_from_str(name: &str, source: &str) -> Result<Config> {
    let dialect = Dialect {
        enable_types: DialectTypes::Enable,
        ..Dialect::Standard
    };
    let ast = AstModule::parse(name, source.to_owned(), &dialect)
        .map_err(|error| anyhow!("{error}"))
        .with_context(|| format!("failed to parse Starlark config {name}"))?;
    let globals = starlark_globals();
    let store = ConfigStore::default();

    Module::with_temp_heap(|module| {
        let mut eval = Evaluator::new(&module);
        eval.extra = Some(&store);
        eval.eval_module(ast, &globals)?;
        starlark::Result::Ok(())
    })
    .map_err(|error| anyhow!("{error}"))
    .with_context(|| format!("failed to evaluate Starlark config {name}"))?;

    store.into_config()
}

#[starlark_module]
#[starlark_types(
    StarlarkGitRepository as GitRepository,
    StarlarkFileSet as FileSet,
    StarlarkTransformation as Transformation,
    StarlarkAuthoring as Authoring
)]
fn codesync_types_api(builder: &mut GlobalsBuilder) {}

#[starlark_module]
fn codesync_config_api(builder: &mut GlobalsBuilder) {
    fn git_origin(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref.or(fetch).unwrap_or_else(|| "HEAD".to_owned()),
            None,
            None,
        ))
    }

    fn git_destination(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
        push: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref.or(fetch).unwrap_or_else(|| "main".to_owned()),
            push,
            None,
        ))
    }

    fn glob(include: Value, exclude: Option<Value>) -> anyhow::Result<StarlarkFileSet> {
        Ok(StarlarkFileSet(FileSet {
            include: strings_from_value(include, "glob include")?,
            exclude: match exclude {
                Some(value) => strings_from_value(value, "glob exclude")?,
                None => Vec::new(),
            },
        }))
    }

    fn r#move(from: String, to: String) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Move { from, to }))
    }

    fn copy(
        from: String,
        to: String,
        paths: Option<&StarlarkFileSet>,
    ) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Copy {
            from,
            to,
            paths: match paths {
                Some(value) => value.0.clone(),
                None => FileSet::all(),
            },
        }))
    }

    fn rename(before: String, after: String) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Rename {
            before,
            after,
        }))
    }

    fn remove(paths: &StarlarkFileSet) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Remove {
            paths: paths.0.clone(),
        }))
    }

    fn strip(paths: &StarlarkFileSet) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Strip {
            paths: paths.0.clone(),
        }))
    }

    fn replace(
        before: String,
        after: String,
        paths: Option<&StarlarkFileSet>,
    ) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Replace {
            before,
            after,
            paths: match paths {
                Some(value) => value.0.clone(),
                None => FileSet::all(),
            },
        }))
    }

    fn workflow(
        name: String,
        origin: &StarlarkGitRepository,
        destination: &StarlarkGitRepository,
        origin_files: Option<&StarlarkFileSet>,
        destination_files: Option<&StarlarkFileSet>,
        transformations: Option<UnpackListOrTuple<&StarlarkTransformation>>,
        first_commit: Option<String>,
        mode: Option<String>,
        authoring: Option<&StarlarkAuthoring>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        push_workflow(
            eval,
            name,
            origin,
            destination,
            origin_files,
            destination_files,
            transformations,
            first_commit,
            mode,
            authoring,
        )?;

        Ok(NoneType)
    }
}

#[starlark_module]
fn codesync_core_api(builder: &mut GlobalsBuilder) {
    fn workflow(
        name: String,
        origin: &StarlarkGitRepository,
        destination: &StarlarkGitRepository,
        origin_files: Option<&StarlarkFileSet>,
        destination_files: Option<&StarlarkFileSet>,
        transformations: Option<UnpackListOrTuple<&StarlarkTransformation>>,
        first_commit: Option<String>,
        mode: Option<String>,
        authoring: Option<&StarlarkAuthoring>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        push_workflow(
            eval,
            name,
            origin,
            destination,
            origin_files,
            destination_files,
            transformations,
            first_commit,
            mode,
            authoring,
        )?;
        Ok(NoneType)
    }

    fn glob(include: Value, exclude: Option<Value>) -> anyhow::Result<StarlarkFileSet> {
        Ok(StarlarkFileSet(FileSet {
            include: strings_from_value(include, "core.glob include")?,
            exclude: match exclude {
                Some(value) => strings_from_value(value, "core.glob exclude")?,
                None => Vec::new(),
            },
        }))
    }

    fn r#move(from: String, to: String) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Move { from, to }))
    }

    fn copy(
        from: String,
        to: String,
        paths: Option<&StarlarkFileSet>,
    ) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Copy {
            from,
            to,
            paths: match paths {
                Some(value) => value.0.clone(),
                None => FileSet::all(),
            },
        }))
    }

    fn rename(before: String, after: String) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Rename {
            before,
            after,
        }))
    }

    fn remove(paths: &StarlarkFileSet) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Remove {
            paths: paths.0.clone(),
        }))
    }

    fn strip(paths: &StarlarkFileSet) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Strip {
            paths: paths.0.clone(),
        }))
    }

    fn replace(
        before: String,
        after: String,
        paths: Option<&StarlarkFileSet>,
    ) -> anyhow::Result<StarlarkTransformation> {
        Ok(StarlarkTransformation(Transformation::Replace {
            before,
            after,
            paths: match paths {
                Some(value) => value.0.clone(),
                None => FileSet::all(),
            },
        }))
    }
}

#[starlark_module]
fn codesync_git_api(builder: &mut GlobalsBuilder) {
    fn origin(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref.or(fetch).unwrap_or_else(|| "HEAD".to_owned()),
            None,
            None,
        ))
    }

    fn github_origin(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref.or(fetch).unwrap_or_else(|| "HEAD".to_owned()),
            None,
            None,
        ))
    }

    fn destination(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
        push: Option<String>,
        destination_ref: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref
                .or(fetch)
                .or(destination_ref)
                .unwrap_or_else(|| "main".to_owned()),
            push,
            None,
        ))
    }

    fn gerrit_destination(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
        push: Option<String>,
        destination_ref: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        Ok(starlark_git_repository(
            url,
            r#ref
                .or(fetch)
                .or(destination_ref)
                .unwrap_or_else(|| "main".to_owned()),
            push,
            None,
        ))
    }

    fn github_destination(
        url: String,
        r#ref: Option<String>,
        fetch: Option<String>,
        push: Option<String>,
        destination_ref: Option<String>,
        pr_branch: Option<String>,
        title: Option<String>,
        body: Option<String>,
        assignees: Option<Value>,
        labels: Option<Value>,
        draft: Option<bool>,
        update_description: Option<bool>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        let push_reference = push.or_else(|| pr_branch.clone());
        Ok(starlark_git_repository(
            url,
            r#ref
                .or(fetch)
                .or(destination_ref)
                .unwrap_or_else(|| "main".to_owned()),
            push_reference,
            Some(PullRequestOptions {
                branch: pr_branch,
                title,
                body,
                assignees: optional_strings_from_value(assignees, "github_destination assignees")?,
                labels: optional_strings_from_value(labels, "github_destination labels")?,
                draft: draft.unwrap_or(false),
                update_description: update_description.unwrap_or(false),
            }),
        ))
    }

    fn github_pr_destination(
        url: String,
        destination_ref: Option<String>,
        r#ref: Option<String>,
        fetch: Option<String>,
        push: Option<String>,
        pr_branch: Option<String>,
        title: Option<String>,
        body: Option<String>,
        assignees: Option<Value>,
        labels: Option<Value>,
        draft: Option<bool>,
        update_description: Option<bool>,
        partial_fetch: Option<bool>,
        allow_empty_diff: Option<bool>,
        allow_empty_diff_merge_statuses: Option<Value>,
        allow_empty_diff_check_suites_to_conclusion: Option<Value>,
        integrates: Option<Value>,
        api_checker: Option<Value>,
        primary_branch_migration: Option<bool>,
        checker: Option<Value>,
        credentials: Option<Value>,
        github_host_name: Option<String>,
    ) -> anyhow::Result<StarlarkGitRepository> {
        let _ = (
            partial_fetch,
            allow_empty_diff,
            allow_empty_diff_merge_statuses,
            allow_empty_diff_check_suites_to_conclusion,
            integrates,
            api_checker,
            primary_branch_migration,
            checker,
            credentials,
            github_host_name,
        );
        let push_reference = push.or_else(|| pr_branch.clone());
        Ok(starlark_git_repository(
            url,
            r#ref
                .or(fetch)
                .or(destination_ref)
                .unwrap_or_else(|| "main".to_owned()),
            push_reference,
            Some(PullRequestOptions {
                branch: pr_branch,
                title,
                body,
                assignees: optional_strings_from_value(
                    assignees,
                    "github_pr_destination assignees",
                )?,
                labels: optional_strings_from_value(labels, "github_pr_destination labels")?,
                draft: draft.unwrap_or(false),
                update_description: update_description.unwrap_or(false),
            }),
        ))
    }
}

#[starlark_module]
fn codesync_authoring_api(builder: &mut GlobalsBuilder) {
    fn pass_thru(default: Option<String>) -> anyhow::Result<StarlarkAuthoring> {
        Ok(StarlarkAuthoring(Authoring {
            mode: AuthoringMode::PassThru,
            default_author: default,
            allowlist: Vec::new(),
        }))
    }

    fn overwrite(default: String) -> anyhow::Result<StarlarkAuthoring> {
        Ok(StarlarkAuthoring(Authoring {
            mode: AuthoringMode::Overwrite,
            default_author: Some(default),
            allowlist: Vec::new(),
        }))
    }

    fn allowed(default: String, allowlist: Value) -> anyhow::Result<StarlarkAuthoring> {
        Ok(StarlarkAuthoring(Authoring {
            mode: AuthoringMode::Allowed,
            default_author: Some(default),
            allowlist: strings_from_value(allowlist, "authoring.allowed allowlist")?,
        }))
    }
}

#[starlark_module]
fn codesync_metadata_api(builder: &mut GlobalsBuilder) {
    fn squash_notes(
        prefix: Option<String>,
        max: Option<i32>,
        compact: Option<bool>,
        show_ref: Option<bool>,
        show_author: Option<bool>,
        show_description: Option<bool>,
        oldest_first: Option<bool>,
        use_merge: Option<bool>,
    ) -> anyhow::Result<StarlarkTransformation> {
        let _ = use_merge;
        let max = max.unwrap_or(100);
        if max < 0 {
            bail!("metadata.squash_notes max must not be negative");
        }
        Ok(StarlarkTransformation(
            Transformation::MetadataSquashNotes {
                prefix: prefix.unwrap_or_else(|| "Copybara import of the project:\n\n".to_owned()),
                max: max as usize,
                compact: compact.unwrap_or(true),
                show_ref: show_ref.unwrap_or(true),
                show_author: show_author.unwrap_or(true),
                show_description: show_description.unwrap_or(true),
                oldest_first: oldest_first.unwrap_or(false),
            },
        ))
    }

    fn public_commit_messages(
        begin_marker: Option<String>,
        end_marker: Option<String>,
        fallback: Option<String>,
    ) -> anyhow::Result<StarlarkTransformation> {
        let begin_marker = begin_marker.unwrap_or_else(|| "<BEGIN_PUBLIC>".to_owned());
        let end_marker = end_marker.unwrap_or_else(|| "<END_PUBLIC>".to_owned());
        if begin_marker.is_empty() {
            bail!("metadata.public_commit_messages begin_marker must not be empty");
        }
        if end_marker.is_empty() {
            bail!("metadata.public_commit_messages end_marker must not be empty");
        }
        Ok(StarlarkTransformation(
            Transformation::PublicCommitMessages {
                begin_marker,
                end_marker,
                fallback: fallback.unwrap_or_else(|| "Internal Change.".to_owned()),
            },
        ))
    }
}

#[starlark_module]
fn codesync_service_api(builder: &mut GlobalsBuilder) {
    fn migration(config: Option<Value>) -> anyhow::Result<NoneType> {
        let _ = config;
        Ok(NoneType)
    }

    fn notifications(config: Option<Value>) -> anyhow::Result<NoneType> {
        let _ = config;
        Ok(NoneType)
    }

    fn email(config: Option<Value>) -> anyhow::Result<NoneType> {
        let _ = config;
        Ok(NoneType)
    }
}

fn push_workflow(
    eval: &mut Evaluator,
    name: String,
    origin: &StarlarkGitRepository,
    destination: &StarlarkGitRepository,
    origin_files: Option<&StarlarkFileSet>,
    destination_files: Option<&StarlarkFileSet>,
    transformations: Option<UnpackListOrTuple<&StarlarkTransformation>>,
    first_commit: Option<String>,
    mode: Option<String>,
    authoring: Option<&StarlarkAuthoring>,
) -> Result<()> {
    let store = eval
        .extra
        .ok_or_else(|| anyhow!("missing config store"))?
        .downcast_ref::<ConfigStore>()
        .ok_or_else(|| anyhow!("invalid config store"))?;

    let transformations = transformations
        .map(|transformations| {
            transformations
                .items
                .into_iter()
                .map(|transformation| transformation.0.clone())
                .collect()
        })
        .unwrap_or_default();

    store.push(Workflow {
        name,
        origin: origin.0.clone(),
        destination: destination.0.clone(),
        destination_files: match destination_files {
            Some(files) => files.0.clone(),
            None => FileSet::all(),
        },
        transformations,
        first_commit,
        origin_files: origin_files.map(|files| files.0.clone()),
        mode: parse_workflow_mode(mode.as_deref())?,
        authoring: authoring.map(|authoring| authoring.0.clone()),
    });

    Ok(())
}

fn starlark_git_repository(
    url: String,
    reference: String,
    push_reference: Option<String>,
    pull_request: Option<PullRequestOptions>,
) -> StarlarkGitRepository {
    StarlarkGitRepository(GitRepository {
        url,
        reference,
        push_reference,
        pull_request,
    })
}

fn parse_workflow_mode(mode: Option<&str>) -> Result<WorkflowMode> {
    match mode.unwrap_or("ITERATIVE") {
        "ITERATIVE" => Ok(WorkflowMode::Iterative),
        "SQUASH" => Ok(WorkflowMode::Squash),
        other => bail!("unsupported workflow mode {other}; expected ITERATIVE or SQUASH"),
    }
}

fn strings_from_value(value: Value, name: &str) -> Result<Vec<String>> {
    if let Some(text) = value.unpack_str() {
        return Ok(vec![text.to_owned()]);
    }

    let json = value.to_json().map_err(|error| anyhow!("{error}"))?;
    serde_json::from_str::<Vec<String>>(&json)
        .with_context(|| format!("{name} must be a string or list of strings"))
}

fn optional_strings_from_value(value: Option<Value>, name: &str) -> Result<Vec<String>> {
    match value {
        Some(value) => strings_from_value(value, name),
        None => Ok(Vec::new()),
    }
}
