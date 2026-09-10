use super::{
    SETTING_AUTO_DOWNLOAD, SETTING_AUTO_DOWNLOAD_CATEGORY, SETTING_AVIF_WORKERS,
    SETTING_CACHE_MEMORY, SETTING_DOWNLOAD_CONCURRENT_CHAPTERS,
    SETTING_DOWNLOAD_PAGE_FETCH_CONCURRENCY, SETTING_DOWNLOAD_PATH, SETTING_LIBRARY_CATEGORIES,
    SETTING_MAX_DOWNLOAD_STORAGE, SETTING_UPDATE_INTERVAL, UPDATE_INTERVAL_CHOICES,
};

pub(super) fn normalize_setting_value(
    setting: &str,
    value: &str,
) -> std::result::Result<String, String> {
    let value = value.trim();
    match setting {
        SETTING_UPDATE_INTERVAL => normalize_update_interval(value),
        SETTING_AUTO_DOWNLOAD => normalize_bool(value),
        SETTING_AUTO_DOWNLOAD_CATEGORY | SETTING_LIBRARY_CATEGORIES => Ok(value.to_owned()),
        SETTING_MAX_DOWNLOAD_STORAGE => normalize_optional_setting(value, validate_optional_u64),
        SETTING_DOWNLOAD_PATH => normalize_required_string(value),
        SETTING_CACHE_MEMORY => normalize_required_u64(value),
        SETTING_AVIF_WORKERS
        | SETTING_DOWNLOAD_CONCURRENT_CHAPTERS
        | SETTING_DOWNLOAD_PAGE_FETCH_CONCURRENCY => normalize_positive_usize(value),
        _ => Err(format!("`{setting}` is not editable from Discord.")),
    }
}

fn normalize_update_interval(value: &str) -> std::result::Result<String, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "Update interval must be a number of hours.".to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err("Update interval must be zero or a positive number of hours.".to_owned());
    }
    Ok(value.to_owned())
}

fn normalize_bool(value: &str) -> std::result::Result<String, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok("true".to_owned()),
        "false" | "no" | "off" | "0" => Ok("false".to_owned()),
        _ => Err("Boolean settings accept true or false.".to_owned()),
    }
}

fn normalize_optional_setting(
    value: &str,
    validate: fn(&str) -> std::result::Result<(), String>,
) -> std::result::Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    validate(value)?;
    Ok(value.to_owned())
}

fn validate_optional_u64(value: &str) -> std::result::Result<(), String> {
    value
        .parse::<u64>()
        .map(|_| ())
        .map_err(|_| "Storage byte settings must be empty or an unsigned integer.".to_owned())
}

fn normalize_required_u64(value: &str) -> std::result::Result<String, String> {
    validate_optional_u64(value)?;
    Ok(value.to_owned())
}

fn normalize_positive_usize(value: &str) -> std::result::Result<String, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| "Worker count must be a positive integer.".to_owned())?;
    if parsed == 0 {
        return Err("Worker count must be greater than zero.".to_owned());
    }
    Ok(parsed.to_string())
}

fn normalize_required_string(value: &str) -> std::result::Result<String, String> {
    if value.is_empty() {
        Err("This setting cannot be empty.".to_owned())
    } else {
        Ok(value.to_owned())
    }
}

pub(super) fn next_update_interval(current: &str) -> &'static str {
    let position = UPDATE_INTERVAL_CHOICES
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(2);
    UPDATE_INTERVAL_CHOICES[(position + 1) % UPDATE_INTERVAL_CHOICES.len()]
}
