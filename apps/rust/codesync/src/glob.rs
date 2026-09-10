use std::path::Path;

use crate::FileSet;

pub fn matches_file_set(file_set: &FileSet, path: &Path) -> bool {
    let path = normalize_path(path);
    file_set
        .include
        .iter()
        .any(|pattern| matches_pattern(pattern, &path))
        && !file_set
            .exclude
            .iter()
            .any(|pattern| matches_pattern(pattern, &path))
}

pub fn matches_pattern(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_matches('/');
    let path = path.trim_matches('/');

    if pattern == "**" {
        return true;
    }

    let pattern_parts = split_path(pattern);
    let path_parts = split_path(path);
    matches_segments(&pattern_parts, &path_parts)
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn split_path(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .collect()
    }
}

fn matches_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((segment, rest)) if *segment == "**" => {
            if rest.is_empty() {
                return true;
            }
            (0..=path.len()).any(|index| matches_segments(rest, &path[index..]))
        }
        Some((segment, rest)) => {
            let Some((path_segment, path_rest)) = path.split_first() else {
                return false;
            };
            matches_component(segment, path_segment) && matches_segments(rest, path_rest)
        }
    }
}

fn matches_component(pattern: &str, text: &str) -> bool {
    matches_component_bytes(pattern.as_bytes(), text.as_bytes())
}

fn matches_component_bytes(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => {
            matches_component_bytes(rest, text)
                || (!text.is_empty() && matches_component_bytes(pattern, &text[1..]))
        }
        Some((b'?', rest)) => !text.is_empty() && matches_component_bytes(rest, &text[1..]),
        Some((byte, rest)) => text.first().is_some_and(|text_byte| {
            text_byte == byte && matches_component_bytes(rest, &text[1..])
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_star_matches_across_directories() {
        assert!(matches_pattern(
            "third_party/demo/**",
            "third_party/demo/src/lib.rs"
        ));
        assert!(matches_pattern("**/*.rs", "src/lib.rs"));
        assert!(matches_pattern("**", "src/lib.rs"));
        assert!(!matches_pattern("*.rs", "src/lib.rs"));
    }

    #[test]
    fn file_set_excludes_take_precedence() {
        let file_set = FileSet {
            include: vec!["**/*.rs".to_owned()],
            exclude: vec!["target/**".to_owned()],
        };

        assert!(matches_file_set(&file_set, Path::new("src/lib.rs")));
        assert!(!matches_file_set(
            &file_set,
            Path::new("target/generated.rs")
        ));
    }
}
