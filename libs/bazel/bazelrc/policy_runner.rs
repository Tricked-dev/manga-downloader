use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Config {
    output: String,
    policy: String,
    inputs: Vec<String>,
    version_inputs: Vec<String>,
    external_label_inputs: Vec<String>,
    yaml_entrypoint_inputs: Vec<String>,
    duplicate_inputs: Vec<String>,
    requires: Vec<Check>,
    forbids: Vec<Check>,
    no_adjacent_duplicate_nonempty: bool,
    allow_only_basename: Option<String>,
}

struct Check {
    name: String,
    snippet: String,
}

struct Source {
    path: String,
    content: String,
}

fn run() -> Result<(), String> {
    let config = Config::parse(env::args().skip(1))?;
    config.validate()?;

    let sources = read_sources(&config.inputs)?;
    let combined = sources
        .iter()
        .map(|source| source.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    for check in &config.requires {
        if !combined.contains(&check.snippet) {
            return Err(format!("missing required {}: {}", config.policy, check.name));
        }
    }

    for check in &config.forbids {
        let matches = find_literal_matches(&sources, &check.snippet);
        if !matches.is_empty() {
            for matched in &matches {
                eprintln!("{matched}");
            }
            return Err(format!(
                "forbidden {} found: {}",
                config.policy, check.name
            ));
        }
    }

    if config.no_adjacent_duplicate_nonempty {
        let duplicate_sources = if config.duplicate_inputs.is_empty() {
            read_sources(&config.inputs)?
        } else {
            read_sources(&config.duplicate_inputs)?
        };
        reject_adjacent_duplicate_nonempty(&duplicate_sources)?;
    }

    if let Some(basename) = &config.allow_only_basename {
        reject_other_basenames(&config.inputs, basename)?;
    }

    reject_version_pins(&read_sources(&config.version_inputs)?)?;
    reject_version_stamped_external_labels(&read_sources(&config.external_label_inputs)?)?;
    reject_yaml_entrypoint_or_cmd(&read_sources(&config.yaml_entrypoint_inputs)?)?;

    fs::write(&config.output, "ok\n")
        .map_err(|err| format!("failed to write {}: {err}", config.output))?;
    Ok(())
}

impl Config {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut config = Config::default();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--output" => config.output = next(&mut args, "--output")?,
                "--policy" => config.policy = next(&mut args, "--policy")?,
                "--input" => config.inputs.push(next(&mut args, "--input")?),
                "--version-input" => config.version_inputs.push(next(&mut args, "--version-input")?),
                "--external-label-input" => config
                    .external_label_inputs
                    .push(next(&mut args, "--external-label-input")?),
                "--yaml-entrypoint-input" => config
                    .yaml_entrypoint_inputs
                    .push(next(&mut args, "--yaml-entrypoint-input")?),
                "--duplicate-input" => config.duplicate_inputs.push(next(&mut args, "--duplicate-input")?),
                "--require" => config.requires.push(parse_check(next(&mut args, "--require")?)?),
                "--forbid" => config.forbids.push(parse_check(next(&mut args, "--forbid")?)?),
                "--no-adjacent-duplicate-nonempty" => {
                    config.no_adjacent_duplicate_nonempty = true;
                }
                "--allow-only-basename" => {
                    config.allow_only_basename = Some(next(&mut args, "--allow-only-basename")?);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.output.is_empty() {
            return Err("missing --output".to_owned());
        }
        if self.policy.is_empty() {
            return Err("missing --policy".to_owned());
        }
        Ok(())
    }
}

fn next(args: &mut impl Iterator<Item = String>, flag: &'static str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_check(encoded: String) -> Result<Check, String> {
    let Some((name, snippet)) = encoded.split_once('\t') else {
        return Err(format!("invalid check encoding: {encoded}"));
    };
    Ok(Check {
        name: name.to_owned(),
        snippet: snippet.to_owned(),
    })
}

fn read_sources(paths: &[String]) -> Result<Vec<Source>, String> {
    paths
        .iter()
        .map(|path| {
            let content =
                fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))?;
            Ok(Source {
                path: path.clone(),
                content,
            })
        })
        .collect()
}

fn find_literal_matches(sources: &[Source], snippet: &str) -> Vec<String> {
    sources
        .iter()
        .flat_map(|source| {
            source
                .content
                .lines()
                .enumerate()
                .filter_map(move |(index, line)| {
                    line.contains(snippet).then(|| {
                        format!("{}:{}:{}", source.path, index + 1, line)
                    })
                })
        })
        .collect()
}

fn reject_adjacent_duplicate_nonempty(sources: &[Source]) -> Result<(), String> {
    let mut previous: Option<&str> = None;

    for source in sources {
        for (index, line) in source.content.lines().enumerate() {
            if previous == Some(line) && !line.trim().is_empty() {
                return Err(format!(
                    "adjacent duplicate non-empty line in {}:{}:{}",
                    source.path,
                    index + 1,
                    line
                ));
            }
            previous = Some(line);
        }
    }

    Ok(())
}

fn reject_other_basenames(paths: &[String], allowed: &str) -> Result<(), String> {
    for path in paths {
        let basename = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        if basename != allowed {
            return Err(format!(
                "do not add GitHub Actions workflows; use buildbuddy.yaml push-only workflows\n{path}"
            ));
        }
    }
    Ok(())
}

fn reject_version_pins(sources: &[Source]) -> Result<(), String> {
    reject_matching_lines(
        sources,
        has_build_version_assignment,
        "apps/ and domain libs must not pin exact versions in BUILD files",
    )
}

fn reject_version_stamped_external_labels(sources: &[Source]) -> Result<(), String> {
    reject_matching_lines(
        sources,
        has_version_stamped_external_label,
        "apps/ and domain libs must not depend on version-stamped external labels in BUILD files",
    )
}

fn reject_yaml_entrypoint_or_cmd(sources: &[Source]) -> Result<(), String> {
    reject_matching_lines(
        sources,
        |line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("entrypoint:") || trimmed.starts_with("cmd:")
        },
        "RBE execution images must not set entrypoint or cmd; BuildBuddy does not ignore ENTRYPOINT for action containers",
    )
}

fn reject_matching_lines(
    sources: &[Source],
    matches: impl Fn(&str) -> bool,
    message: &'static str,
) -> Result<(), String> {
    let mut found = false;
    for source in sources {
        for (index, line) in source.content.lines().enumerate() {
            if matches(line) {
                eprintln!("{}:{}:{}", source.path, index + 1, line);
                found = true;
            }
        }
    }
    if found {
        Err(message.to_owned())
    } else {
        Ok(())
    }
}

fn has_build_version_assignment(line: &str) -> bool {
    for (index, _) in line.match_indices("version") {
        let before = line[..index].chars().next_back();
        if before.is_some_and(|character| !character.is_whitespace()) {
            continue;
        }
        let after = &line[index + "version".len()..];
        if after.trim_start().starts_with('=') {
            return true;
        }
    }
    false
}

fn has_version_stamped_external_label(line: &str) -> bool {
    line.split(|character: char| character.is_whitespace() || character == '"')
        .filter_map(|token| {
            let label_start = token.find('@')?;
            let label = &token[label_start..];
            let target_start = label.find("//:")?;
            Some(&label[target_start + 3..])
        })
        .any(contains_digit_dot_digit)
}

fn contains_digit_dot_digit(value: &str) -> bool {
    let chars = value.chars().collect::<Vec<_>>();
    chars
        .windows(3)
        .any(|window| window[0].is_ascii_digit() && window[1] == '.' && window[2].is_ascii_digit())
}
