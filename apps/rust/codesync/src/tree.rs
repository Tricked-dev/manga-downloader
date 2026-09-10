use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{FileSet, Transformation};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyStats {
    pub copied: usize,
    pub removed: usize,
}

pub fn reset_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

pub fn apply_transformations(root: &Path, transformations: &[Transformation]) -> Result<()> {
    for transformation in transformations {
        match transformation {
            Transformation::Copy { from, to, paths } => copy_tree(root, from, to, paths)?,
            Transformation::Move { from, to } => move_tree(root, from, to)?,
            Transformation::Rename { before, after } => move_tree(root, before, after)?,
            Transformation::Remove { paths } => {
                remove_matching_files(root, paths)?;
            }
            Transformation::Strip { paths } => {
                remove_matching_files(root, paths)?;
            }
            Transformation::Replace {
                before,
                after,
                paths,
            } => replace_matching_files(root, paths, before.as_bytes(), after.as_bytes())?,
            Transformation::MetadataSquashNotes { .. } => {}
            Transformation::PublicCommitMessages { .. } => {}
        }
    }
    Ok(())
}

pub fn keep_matching_files(root: &Path, files: &FileSet) -> Result<usize> {
    let mut kept = 0;
    for file in list_files(root)? {
        let relative = file.strip_prefix(root).with_context(|| {
            format!(
                "failed to make {} relative to {}",
                file.display(),
                root.display()
            )
        })?;
        if files.matches(relative) {
            kept += 1;
            continue;
        }
        delete_path(&file)?;
    }
    prune_empty_dirs(root)?;
    Ok(kept)
}

pub fn replace_destination(
    source_root: &Path,
    destination_root: &Path,
    files: &FileSet,
) -> Result<CopyStats> {
    let removed = remove_matching_files(destination_root, files)?;
    let copied = copy_matching_files(source_root, destination_root, files)?;
    Ok(CopyStats { copied, removed })
}

fn move_tree(root: &Path, from: &str, to: &str) -> Result<()> {
    let from = from.trim_matches('/');
    let to = to.trim_matches('/');
    if from == to {
        return Ok(());
    }

    if from.is_empty() {
        return move_root_contents(root, to);
    }

    let source = root.join(from);
    if !source.exists() {
        return Ok(());
    }

    let destination = root.join(to);
    move_path_via_temp(root, &source, &destination)?;
    prune_empty_dirs(root)?;
    Ok(())
}

fn move_root_contents(root: &Path, to: &str) -> Result<()> {
    if to.is_empty() {
        return Ok(());
    }

    let temp = root.with_file_name(format!(".codesync-move-{}", std::process::id()));
    if temp.exists() {
        fs::remove_dir_all(&temp).with_context(|| format!("failed to clean {}", temp.display()))?;
    }
    fs::create_dir_all(&temp).with_context(|| format!("failed to create {}", temp.display()))?;

    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        fs::rename(entry.path(), temp.join(entry.file_name()))
            .with_context(|| format!("failed to move {} into temp tree", entry.path().display()))?;
    }

    let destination = root.join(to);
    fs::create_dir_all(&destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    for entry in
        fs::read_dir(&temp).with_context(|| format!("failed to read {}", temp.display()))?
    {
        let entry = entry?;
        fs::rename(entry.path(), destination.join(entry.file_name())).with_context(|| {
            format!(
                "failed to move {} into {}",
                entry.path().display(),
                destination.display()
            )
        })?;
    }

    fs::remove_dir_all(&temp).with_context(|| format!("failed to remove {}", temp.display()))
}

fn move_path_via_temp(root: &Path, source: &Path, destination: &Path) -> Result<()> {
    let temp = root.join(format!(".codesync-move-tmp-{}", std::process::id()));
    delete_path(&temp)?;
    fs::rename(source, &temp)
        .with_context(|| format!("failed to move {} into temp path", source.display()))?;
    delete_path(destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::rename(&temp, destination).with_context(|| {
        format!(
            "failed to move temp path {} to {}",
            temp.display(),
            destination.display()
        )
    })
}

fn copy_tree(root: &Path, from: &str, to: &str, paths: &FileSet) -> Result<()> {
    let from = from.trim_matches('/');
    let to = to.trim_matches('/');
    let source = root.join(from);
    if !source.exists() {
        return Ok(());
    }

    if source.is_file() || source.is_symlink() {
        let relative = source
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(from));
        if paths.matches(&relative) {
            copy_file(&source, &root.join(to))?;
        }
        return Ok(());
    }

    for source_file in list_files(&source)? {
        let relative = source_file.strip_prefix(&source).with_context(|| {
            format!(
                "failed to make {} relative to {}",
                source_file.display(),
                source.display()
            )
        })?;
        if !paths.matches(relative) {
            continue;
        }
        copy_file(&source_file, &root.join(to).join(relative))?;
    }

    Ok(())
}

pub fn remove_matching_files(root: &Path, files: &FileSet) -> Result<usize> {
    let mut removed = 0;
    for file in list_files(root)? {
        let relative = file.strip_prefix(root).with_context(|| {
            format!(
                "failed to make {} relative to {}",
                file.display(),
                root.display()
            )
        })?;
        if files.matches(relative) {
            delete_path(&file)?;
            removed += 1;
        }
    }
    prune_empty_dirs(root)?;
    Ok(removed)
}

fn replace_matching_files(root: &Path, files: &FileSet, before: &[u8], after: &[u8]) -> Result<()> {
    if before.is_empty() {
        bail!("replace before value must not be empty");
    }

    for file in list_files(root)? {
        let relative = file.strip_prefix(root).with_context(|| {
            format!(
                "failed to make {} relative to {}",
                file.display(),
                root.display()
            )
        })?;
        if !files.matches(relative) {
            continue;
        }

        let content =
            fs::read(&file).with_context(|| format!("failed to read {}", file.display()))?;
        let replaced = replace_bytes(&content, before, after);
        if replaced != content {
            fs::write(&file, replaced)
                .with_context(|| format!("failed to write {}", file.display()))?;
        }
    }

    Ok(())
}

fn copy_matching_files(
    source_root: &Path,
    destination_root: &Path,
    files: &FileSet,
) -> Result<usize> {
    let mut copied = 0;
    for source_file in list_files(source_root)? {
        let relative = source_file.strip_prefix(source_root).with_context(|| {
            format!(
                "failed to make {} relative to {}",
                source_file.display(),
                source_root.display()
            )
        })?;
        if !files.matches(relative) {
            continue;
        }

        let destination_file = destination_root.join(relative);
        copy_file(&source_file, &destination_file)?;
        copied += 1;
    }
    Ok(copied)
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to stat {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(source)
            .with_context(|| format!("failed to read link {}", source.display()))?;
        delete_path(destination)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination).with_context(|| {
            format!(
                "failed to create symlink {} -> {}",
                destination.display(),
                target.display()
            )
        })?;
        #[cfg(not(unix))]
        fs::copy(source, destination)
            .with_context(|| format!("failed to copy {}", source.display()))?;
    } else {
        fs::copy(source, destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    Ok(())
}

fn list_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if !dir.exists() {
            continue;
        }
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }

            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .with_context(|| format!("failed to stat {}", path.display()))?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn delete_path(path: &Path) -> Result<()> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    }
}

fn prune_empty_dirs(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }

    let mut is_empty = true;
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            is_empty = false;
            continue;
        }

        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            if prune_empty_dirs(&path)? {
                fs::remove_dir(&path)
                    .with_context(|| format!("failed to remove empty dir {}", path.display()))?;
            } else {
                is_empty = false;
            }
        } else {
            is_empty = false;
        }
    }

    Ok(is_empty)
}

fn replace_bytes(content: &[u8], before: &[u8], after: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(content.len());
    let mut cursor = 0;

    while let Some(relative_index) = find_subslice(&content[cursor..], before) {
        let index = cursor + relative_index;
        output.extend_from_slice(&content[cursor..index]);
        output.extend_from_slice(after);
        cursor = index + before.len();
    }

    output.extend_from_slice(&content[cursor..]);
    output
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
