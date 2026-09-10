use std::{
    env,
    ffi::OsString,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, ExitCode, ExitStatus},
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
    let script = resolve_runfile(next_arg(&mut args, "missing k6 script runfile path")?);

    let status = Command::new("k6")
        .arg("run")
        .arg(&script)
        .args(args)
        .status();

    match status {
        Ok(status) => Ok(exit_code(status)),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            eprintln!("error: k6 is required on PATH for this bazel run target");
            Ok(127)
        }
        Err(err) => Err(format!("failed to execute k6: {err}")),
    }
}

fn next_arg(
    args: &mut impl Iterator<Item = OsString>,
    message: &'static str,
) -> Result<OsString, String> {
    args.next().ok_or_else(|| message.to_owned())
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

fn exit_code(status: ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return truncate_exit_code(code);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = status.signal() {
            return truncate_exit_code(128 + signal);
        }
    }

    1
}

fn truncate_exit_code(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(1)
}
