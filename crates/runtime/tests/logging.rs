use backend_runtime::truncate_for_log;

#[test]
fn truncate_for_log_leaves_short_values_unchanged() {
    assert_eq!(truncate_for_log("short", 10), "short");
}

#[test]
fn truncate_for_log_respects_character_boundaries() {
    assert_eq!(truncate_for_log("åßç∂", 2), "åß...");
}

#[test]
fn truncate_for_log_marks_non_empty_values_when_limit_is_zero() {
    assert_eq!(truncate_for_log("hidden", 0), "...");
    assert_eq!(truncate_for_log("", 0), "");
}
