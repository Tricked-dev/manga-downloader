use crate::api::dto::ServerBuildInfoResponse;
use shadow_rs::shadow;

shadow!(build);

pub fn server_build_info() -> ServerBuildInfoResponse {
    ServerBuildInfoResponse {
        name: build::PROJECT_NAME.to_string(),
        version: build::PKG_VERSION.to_string(),
        branch: build_value(build::BRANCH).or_else(|| metadata_env("GIT_BRANCH")),
        commit_hash: build_value(build::COMMIT_HASH).or_else(|| metadata_env("GIT_COMMIT_SHA")),
        commit_short_hash: build_value(build::SHORT_COMMIT)
            .or_else(|| metadata_env("GIT_COMMIT_SHORT_SHA")),
        commit_date: build_value(build::COMMIT_DATE_3339)
            .or_else(|| build_value(build::COMMIT_DATE))
            .or_else(|| metadata_env("GIT_COMMIT_AT")),
        build_time: metadata_env("BUILD_TIME_3339")
            .or_else(|| metadata_env("BUILD_TIME"))
            .or_else(|| build_value(build::BUILD_TIME_3339))
            .or_else(|| build_value(build::BUILD_TIME))
            .or_else(|| metadata_env("GIT_COMMIT_AT")),
        build_channel: value_or_none(build::BUILD_RUST_CHANNEL),
        build_target: value_or_none(build::BUILD_TARGET),
        git_clean: metadata_bool("GIT_CLEAN").or(Some(build::GIT_CLEAN)),
    }
}

fn metadata_env(key: &str) -> Option<String> {
    runtime_env(key).or_else(|| compile_env(key))
}

fn runtime_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(|value| value_or_none(&value))
}

fn compile_env(key: &str) -> Option<String> {
    match key {
        "BUILD_TIME" => option_env!("BUILD_TIME").and_then(value_or_none),
        "BUILD_TIME_3339" => option_env!("BUILD_TIME_3339").and_then(value_or_none),
        "GIT_BRANCH" => option_env!("GIT_BRANCH").and_then(value_or_none),
        "GIT_CLEAN" => option_env!("GIT_CLEAN").and_then(value_or_none),
        "GIT_COMMIT_AT" => option_env!("GIT_COMMIT_AT").and_then(value_or_none),
        "GIT_COMMIT_SHA" => option_env!("GIT_COMMIT_SHA").and_then(value_or_none),
        "GIT_COMMIT_SHORT_SHA" => option_env!("GIT_COMMIT_SHORT_SHA").and_then(value_or_none),
        _ => None,
    }
}

fn metadata_bool(key: &str) -> Option<bool> {
    metadata_env(key).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    })
}

fn build_value(value: &str) -> Option<String> {
    let value = value_or_none(value)?;
    if is_reproducible_epoch(&value) {
        None
    } else {
        Some(value)
    }
}

fn is_reproducible_epoch(value: &str) -> bool {
    value.starts_with("1970-01-01T00:00:01")
        || value.starts_with("1970-01-01 00:00:01")
        || value == "1"
}

fn value_or_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(trimmed.to_string())
    }
}
