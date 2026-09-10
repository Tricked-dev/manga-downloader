use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let mut args = env::args_os().skip(1);
    let flamegraph = resolve_runfile(next_arg(&mut args, "missing flamegraph tool path")?);
    let binary = resolve_runfile(next_arg(&mut args, "missing profiled binary path")?);
    let profile_name = next_arg(&mut args, "missing profile name")?;
    let output = output_path(&profile_name)?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }

    eprintln!("writing flamegraph to {}", output.display());
    let mut command = Command::new(flamegraph);
    command
        .arg("--output")
        .arg(&output)
        .arg("--")
        .arg(binary)
        .args(args);

    command_status(command)
}

fn next_arg(
    args: &mut impl Iterator<Item = OsString>,
    message: &'static str,
) -> Result<OsString, String> {
    args.next().ok_or_else(|| message.to_owned())
}

fn output_path(profile_name: &OsString) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("MANGA_FLAMEGRAPH_OUTPUT") {
        return Ok(PathBuf::from(path));
    }

    let workspace = env_path("BUILD_WORKSPACE_DIRECTORY")
        .or_else(|| env::current_dir().ok())
        .ok_or("failed to resolve current workspace directory")?;
    let mut filename = profile_name
        .to_string_lossy()
        .replace(['/', '\\', ':'], "_");
    filename.push_str(".svg");
    Ok(workspace.join(".cache/bazel/flamegraphs").join(filename))
}

fn command_status(mut command: Command) -> Result<u8, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .map_err(|err| format!("failed to execute {program}: {err}"))?;
    Ok(status.code().unwrap_or(1).try_into().unwrap_or(1))
}

fn resolve_runfile(path: OsString) -> PathBuf {
    let relative = PathBuf::from(path);
    if relative.is_absolute() && relative.exists() {
        return relative;
    }

    let mut candidates = Vec::new();
    candidates.push(relative.clone());

    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join(&relative));
    }
    if let Some(workspace) = env_path("BUILD_WORKSPACE_DIRECTORY") {
        candidates.push(workspace.join(&relative));
    }
    for key in ["TEST_SRCDIR", "RUNFILES_DIR"] {
        if let Some(root) = env_path(key) {
            candidates.push(root.join(&relative));
            candidates.push(root.join("_main").join(&relative));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .unwrap_or(relative)
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| Path::new(path).exists())
}
