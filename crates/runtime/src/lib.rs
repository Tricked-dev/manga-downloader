#![allow(clippy::missing_errors_doc)]

use std::{env, io::IsTerminal};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[must_use]
/// Returns whether tracing output should use ANSI colors on stderr.
pub fn color_logs_enabled() -> bool {
    env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

/// Installs the process-wide tracing subscriber.
pub fn init_tracing(default_filter: &str, color_logs: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_ansi(color_logs))
        .init();
}

#[must_use]
/// Truncates a string by Unicode scalar values for compact log fields.
pub fn truncate_for_log(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut iter = value.chars();
    for _ in 0..max_chars {
        match iter.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }

    if iter.next().is_some() {
        out.push_str("...");
    }

    out
}
