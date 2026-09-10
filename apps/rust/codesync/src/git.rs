use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use anyhow::{Context, Result, anyhow, bail};
use gix::{bstr::ByteSlice, refs::transaction::PreviousValue};

use crate::{Authoring, AuthoringMode, GitRepository};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub message: String,
}

impl CommitInfo {
    #[must_use]
    pub fn short_sha(&self) -> &str {
        self.sha.get(..12).unwrap_or(&self.sha)
    }
}

#[derive(Clone, Debug)]
pub struct GitWorktree {
    repo: gix::Repository,
    path: PathBuf,
}

impl GitWorktree {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn clone_for_source(repo: &GitRepository, path: &Path) -> Result<Self> {
        clone_repo(repo, path).map(|repository| Self {
            repo: repository,
            path: path.to_path_buf(),
        })
    }

    pub fn clone_for_destination(repo: &GitRepository, path: &Path) -> Result<Self> {
        clone_repo(repo, path).map(|repository| Self {
            repo: repository,
            path: path.to_path_buf(),
        })
    }

    pub fn source_head(&self) -> Result<String> {
        self.rev_parse("HEAD")
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        Ok(self
            .repo
            .rev_parse_single(rev)
            .with_context(|| format!("failed to resolve {rev}"))?
            .detach()
            .to_string())
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool> {
        let ancestor = self.resolve_commit_id(ancestor)?;
        let descendant = self.resolve_commit_id(descendant)?;
        if ancestor == descendant {
            return Ok(true);
        }

        let mut stack = vec![descendant];
        while let Some(id) = stack.pop() {
            let commit = self.find_commit(id)?;
            for parent in commit.parent_ids() {
                let parent = parent.detach();
                if parent == ancestor {
                    return Ok(true);
                }
                stack.push(parent);
            }
        }

        Ok(false)
    }

    pub fn rev_list_reverse(&self, since_exclusive: &str, head: &str) -> Result<Vec<String>> {
        let since_exclusive = self.resolve_commit_id(since_exclusive)?;
        let mut commits = self.commit_ids_to_root(self.resolve_commit_id(head)?)?;
        commits.reverse();
        Ok(commits
            .into_iter()
            .skip_while(|id| *id != since_exclusive)
            .skip(1)
            .map(|id| id.to_string())
            .collect())
    }

    pub fn rev_list_all_reverse(&self, head: &str) -> Result<Vec<String>> {
        let mut commits = self.commit_ids_to_root(self.resolve_commit_id(head)?)?;
        commits.reverse();
        Ok(commits.into_iter().map(|id| id.to_string()).collect())
    }

    pub fn commit_info(&self, sha: &str) -> Result<CommitInfo> {
        let commit = self.find_commit(self.resolve_commit_id(sha)?)?;
        let message = String::from_utf8_lossy(commit.message_raw()?.as_ref()).into_owned();
        let subject = message
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("sync source commit")
            .to_owned();
        let author = commit.author()?;

        Ok(CommitInfo {
            sha: sha.to_owned(),
            subject,
            author_name: author.name.to_string(),
            author_email: author.email.to_string(),
            message,
        })
    }

    pub fn find_last_synced_rev(&self, workflow: &str) -> Result<Option<String>> {
        let head = match self.repo.head_commit() {
            Ok(commit) => commit.id,
            Err(_) => return Ok(None),
        };

        for id in self.commit_ids_to_root(head)? {
            let message = self.find_commit(id)?.message_raw_sloppy().to_string();
            if !message
                .lines()
                .any(|line| line.trim() == format!("CodeSync-Workflow: {workflow}"))
            {
                continue;
            }

            if let Some(source_rev) = message.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("CodeSync-Source-Rev:")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            }) {
                return Ok(Some(source_rev));
            }
        }

        Ok(None)
    }

    pub fn export_tree(&self, sha: &str, destination: &Path) -> Result<()> {
        fs::create_dir_all(destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
        let commit = self.find_commit(self.resolve_commit_id(sha)?)?;
        export_tree_entries(&commit.tree()?, destination)
    }

    pub fn commit_with_message(
        &self,
        commit: &CommitInfo,
        workflow: &str,
        message: &str,
        authoring: Option<&Authoring>,
    ) -> Result<bool> {
        let tree_id = write_tree_from_dir(&self.repo, &self.path)
            .with_context(|| format!("failed to write tree from {}", self.path.display()))?;
        let head = self.repo.head_commit().ok();
        if head
            .as_ref()
            .and_then(|commit| commit.tree_id().ok())
            .is_some_and(|head_tree| head_tree.detach() == tree_id)
        {
            return Ok(false);
        }

        let mut message = message.trim_end().to_owned();
        message.push_str("\n\nCodeSync-Workflow: ");
        message.push_str(workflow);
        message.push_str("\nCodeSync-Source-Rev: ");
        message.push_str(&commit.sha);
        message.push_str("\nGitOrigin-RevId: ");
        message.push_str(&commit.sha);
        message.push('\n');

        let author = author_identity(commit, authoring)?;
        let author = actor_signature(&author.name, &author.email);
        let committer = actor_signature("CodeSync", "codesync@localhost");
        let parents = head
            .as_ref()
            .map(|commit| vec![commit.id])
            .unwrap_or_default();
        let mut committer_time = gix::actor::date::parse::TimeBuf::default();
        let mut author_time = gix::actor::date::parse::TimeBuf::default();
        self.repo
            .commit_as(
                committer.to_ref(&mut committer_time),
                author.to_ref(&mut author_time),
                "HEAD",
                message,
                tree_id,
                parents,
            )
            .context("failed to create destination commit with gitoxide")?;
        Ok(true)
    }

    pub fn push_head(&self, reference: &str) -> Result<()> {
        let head = self
            .repo
            .head_id()
            .context("destination has no HEAD commit to push")?
            .detach();
        let reference = branch_ref(reference);
        let remote = self
            .repo
            .find_remote("origin")
            .context("destination clone has no origin remote")?;
        let destination = remote
            .url(gix::remote::Direction::Push)
            .or_else(|| remote.url(gix::remote::Direction::Fetch))
            .map(|url| url.to_bstring().to_string())
            .context("origin remote does not have a URL")?;

        let remote_path = Path::new(&destination);
        if !remote_path.exists() {
            bail!(
                "gitoxide push is currently implemented for local path remotes only; \
                 destination origin is {destination}"
            );
        }

        let remote = gix::open(remote_path)
            .with_context(|| format!("failed to open local destination remote {destination}"))?;
        copy_loose_objects(
            &self.repo.git_dir().join("objects"),
            &remote.git_dir().join("objects"),
        )
        .with_context(|| format!("failed to transfer objects into local remote {destination}"))?;
        let constraint = remote
            .find_reference(&reference)
            .ok()
            .and_then(|reference| {
                reference.target().try_id().map(|id| {
                    PreviousValue::MustExistAndMatch(gix::refs::Target::Object(id.to_owned()))
                })
            })
            .unwrap_or(PreviousValue::Any);
        remote
            .reference(reference.as_str(), head, constraint, "codesync push")
            .with_context(|| format!("failed to update local remote ref {reference}"))?;
        Ok(())
    }

    fn resolve_commit_id(&self, rev: &str) -> Result<gix::ObjectId> {
        let id = self
            .repo
            .rev_parse_single(rev)
            .with_context(|| format!("failed to resolve {rev}"))?
            .detach();
        self.find_commit(id)?;
        Ok(id)
    }

    fn find_commit(&self, id: gix::ObjectId) -> Result<gix::Commit<'_>> {
        self.repo
            .find_object(id)
            .with_context(|| format!("failed to find object {id}"))?
            .peel_to_commit()
            .with_context(|| format!("object {id} is not a commit"))
    }

    fn commit_ids_to_root(&self, head: gix::ObjectId) -> Result<Vec<gix::ObjectId>> {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack = vec![head];
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let commit = self.find_commit(id)?;
            out.push(id);
            for parent in commit.parent_ids() {
                stack.push(parent.detach());
            }
        }
        Ok(out)
    }
}

fn copy_loose_objects(source_objects: &Path, destination_objects: &Path) -> Result<usize> {
    let mut copied = 0;
    for entry in fs::read_dir(source_objects)
        .with_context(|| format!("failed to read {}", source_objects.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", source_objects.display()))?;
        let fanout_name = entry.file_name();
        let fanout = fanout_name.to_string_lossy();
        if fanout.len() != 2 || !fanout.chars().all(|ch| ch.is_ascii_hexdigit()) {
            continue;
        }

        let source_fanout = entry.path();
        if !source_fanout.is_dir() {
            continue;
        }
        let destination_fanout = destination_objects.join(&fanout_name);
        fs::create_dir_all(&destination_fanout)
            .with_context(|| format!("failed to create {}", destination_fanout.display()))?;

        for object in fs::read_dir(&source_fanout)
            .with_context(|| format!("failed to read {}", source_fanout.display()))?
        {
            let object = object
                .with_context(|| format!("failed to read entry in {}", source_fanout.display()))?;
            let source = object.path();
            if !source.is_file() {
                continue;
            }
            let destination = destination_fanout.join(object.file_name());
            if destination.exists() {
                continue;
            }
            fs::copy(&source, &destination).with_context(|| {
                format!(
                    "failed to copy object {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
            copied += 1;
        }
    }
    Ok(copied)
}

struct AuthorIdentity {
    name: String,
    email: String,
}

fn author_identity(commit: &CommitInfo, authoring: Option<&Authoring>) -> Result<AuthorIdentity> {
    let original = AuthorIdentity {
        name: commit.author_name.clone(),
        email: commit.author_email.clone(),
    };
    let Some(authoring) = authoring else {
        return Ok(original);
    };

    match authoring.mode {
        AuthoringMode::PassThru => Ok(original),
        AuthoringMode::Overwrite => authoring
            .default_author
            .as_deref()
            .map(parse_author)
            .transpose()?
            .map_or(Ok(original), Ok),
        AuthoringMode::Allowed => {
            if authoring.allowlist.iter().any(|allowed| {
                allowed == &original.email
                    || allowed == &format!("{} <{}>", original.name, original.email)
            }) {
                Ok(original)
            } else {
                authoring
                    .default_author
                    .as_deref()
                    .map(parse_author)
                    .transpose()?
                    .map_or(Ok(original), Ok)
            }
        }
    }
}

fn parse_author(author: &str) -> Result<AuthorIdentity> {
    let Some((name, email)) = author.rsplit_once('<') else {
        bail!("author must use 'Name <email>' format: {author}");
    };
    let Some(email) = email.trim().strip_suffix('>') else {
        bail!("author must use 'Name <email>' format: {author}");
    };
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() || email.is_empty() {
        bail!("author must include a non-empty name and email: {author}");
    }
    Ok(AuthorIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
    })
}

fn clone_repo(repo: &GitRepository, path: &Path) -> Result<gix::Repository> {
    let should_interrupt = AtomicBool::new(false);
    let mut prepare = gix::clone::PrepareFetch::new(
        repo.url.as_str(),
        path,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .with_context(|| format!("failed to initialize clone of {}", repo.url))?
    .with_ref_name(Some(repo.reference.as_str()))
    .map_err(|error| anyhow!("invalid git reference {}: {error}", repo.reference))?;
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &should_interrupt)
        .with_context(|| format!("failed to fetch {}", repo.url))?;
    let (repository, _) = checkout
        .main_worktree(gix::progress::Discard, &should_interrupt)
        .with_context(|| format!("failed to check out {}", repo.reference))?;
    Ok(repository)
}

fn export_tree_entries(tree: &gix::Tree<'_>, destination: &Path) -> Result<()> {
    for entry in tree.iter() {
        let entry = entry.context("failed to decode tree entry")?;
        let entry_path = destination.join(path_component(entry.filename())?);
        match entry.kind() {
            gix::objs::tree::EntryKind::Tree => {
                fs::create_dir_all(&entry_path)
                    .with_context(|| format!("failed to create {}", entry_path.display()))?;
                export_tree_entries(
                    &entry.object()?.try_into_tree().map_err(|_| {
                        anyhow!("tree entry {} did not point to a tree", entry.filename())
                    })?,
                    &entry_path,
                )?;
            }
            gix::objs::tree::EntryKind::Blob | gix::objs::tree::EntryKind::BlobExecutable => {
                if let Some(parent) = entry_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let blob = entry.object()?.try_into_blob().map_err(|_| {
                    anyhow!("tree entry {} did not point to a blob", entry.filename())
                })?;
                fs::write(&entry_path, &blob.data)
                    .with_context(|| format!("failed to write {}", entry_path.display()))?;
            }
            gix::objs::tree::EntryKind::Link => {
                if let Some(parent) = entry_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let blob = entry.object()?.try_into_blob().map_err(|_| {
                    anyhow!(
                        "tree entry {} did not point to a symlink blob",
                        entry.filename()
                    )
                })?;
                write_symlink(&entry_path, &String::from_utf8_lossy(&blob.data))?;
            }
            gix::objs::tree::EntryKind::Commit => {
                bail!(
                    "submodule entry {} cannot be exported yet",
                    entry.filename()
                );
            }
        }
    }
    Ok(())
}

fn write_tree_from_dir(repo: &gix::Repository, dir: &Path) -> Result<gix::ObjectId> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;

        let (mode, oid) = if file_type.is_dir() {
            (
                gix::objs::tree::EntryKind::Tree.into(),
                write_tree_from_dir(repo, &path)?,
            )
        } else if file_type.is_symlink() {
            (
                gix::objs::tree::EntryKind::Link.into(),
                repo.write_blob(read_link_bytes(&path)?)
                    .with_context(|| format!("failed to write symlink blob {}", path.display()))?
                    .detach(),
            )
        } else {
            let mode = if is_executable(&path)? {
                gix::objs::tree::EntryKind::BlobExecutable.into()
            } else {
                gix::objs::tree::EntryKind::Blob.into()
            };
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            (
                mode,
                repo.write_blob(bytes)
                    .with_context(|| format!("failed to write blob {}", path.display()))?
                    .detach(),
            )
        };

        entries.push(gix::objs::tree::Entry {
            mode,
            filename: os_str_bytes(&name).into(),
            oid,
        });
    }
    entries.sort();
    Ok(repo
        .write_object(gix::objs::Tree { entries })
        .context("failed to write tree object")?
        .detach())
}

fn actor_signature(name: &str, email: &str) -> gix::actor::Signature {
    gix::actor::Signature {
        name: name.as_bytes().into(),
        email: email.as_bytes().into(),
        time: gix::actor::date::Time::now_local_or_utc(),
    }
}

fn branch_ref(reference: &str) -> String {
    if reference.starts_with("refs/") {
        reference.to_owned()
    } else {
        format!("refs/heads/{reference}")
    }
}

fn path_component(name: &gix::bstr::BStr) -> Result<PathBuf> {
    use std::ffi::OsStr;

    let component = name.to_os_str_lossy();
    let component_os: &OsStr = component.as_ref();
    if component_os.is_empty()
        || component_os == OsStr::new(".")
        || component_os == OsStr::new("..")
    {
        bail!("invalid git tree path component {name:?}");
    }
    Ok(PathBuf::from(component.into_owned()))
}

fn os_str_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        value.to_string_lossy().as_bytes().to_vec()
    }
}

fn read_link_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read_link(path)
        .map(|target| os_str_bytes(target.as_os_str()))
        .with_context(|| format!("failed to read symlink {}", path.display()))
}

fn write_symlink(path: &Path, target: &str) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, path)
            .with_context(|| format!("failed to create symlink {}", path.display()))
    }
    #[cfg(not(unix))]
    {
        fs::write(path, target).with_context(|| {
            format!(
                "failed to materialize symlink {} as a plain file",
                path.display()
            )
        })
    }
}

fn is_executable(path: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?
            .permissions()
            .mode()
            & 0o111
            != 0)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(false)
    }
}
