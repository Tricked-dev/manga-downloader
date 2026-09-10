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
    let mode = args.next().ok_or("missing cargo tool mode")?;

    match mode.to_string_lossy().as_ref() {
        "deny" => {
            let tool = resolve_runfile(next_arg(&mut args, "missing cargo-deny tool path")?);
            let manifest = resolve_runfile(next_arg(&mut args, "missing Cargo.toml path")?);
            let mut command = Command::new(tool);
            command
                .arg("--manifest-path")
                .arg(manifest)
                .arg("check")
                .args(args);
            command_status(command)
        }
        "shear" => {
            let tool = resolve_runfile(next_arg(&mut args, "missing cargo-shear tool path")?);
            let workspace = resolve_runfile(next_arg(&mut args, "missing workspace path")?);
            if !workspace.join("Cargo.toml").exists() {
                let mut command = Command::new(tool);
                command.arg("--version");
                return command_status(command);
            }
            let mut command = Command::new(tool);
            command
                .arg("--locked")
                .arg("--deny-warnings")
                .args(args)
                .arg(workspace);
            command_status(command)
        }
        other => Err(format!("unknown cargo tool mode: {other}")),
    }
}

fn next_arg(
    args: &mut impl Iterator<Item = OsString>,
    message: &'static str,
) -> Result<OsString, String> {
    args.next().ok_or_else(|| message.to_owned())
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
