use std::path::Path as StdPath;

use anyhow::Result;
use backend_core::is_safe_filename_component;

pub(super) fn sanitize_plugin_filename(file_name: &str) -> Result<String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Plugin filename is empty");
    }
    if !StdPath::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wasm"))
    {
        anyhow::bail!("Plugin filename must end with .wasm");
    }
    if !is_safe_filename_component(trimmed) {
        anyhow::bail!("Plugin filename must not contain unsafe filename characters");
    }

    Ok(trimmed.to_string())
}

pub(super) fn is_wasm_file(path: &StdPath) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wasm"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_filename_sanitizer_accepts_trimmed_wasm_filenames() {
        assert_eq!(
            sanitize_plugin_filename("  source-plugin.WASM  ").expect("filename should sanitize"),
            "source-plugin.WASM"
        );
    }

    #[test]
    fn plugin_filename_sanitizer_rejects_empty_non_wasm_and_path_names() {
        for file_name in [
            "",
            "source-plugin.zip",
            "../source-plugin.wasm",
            "nested/source-plugin.wasm",
            "nested\\source-plugin.wasm",
        ] {
            assert!(
                sanitize_plugin_filename(file_name).is_err(),
                "{file_name:?} should be rejected"
            );
        }
    }
}
