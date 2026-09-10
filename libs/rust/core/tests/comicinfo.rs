use backend_core::{
    ComicInfoChapterMetadata, ComicInfoCredits, build_comicinfo_xml, download_archive_filename,
    download_archive_path, is_avif_content_type, is_safe_filename_component,
    is_textual_content_type, parse_byte_size, parse_positive_byte_size, primary_content_type,
    reencode_file_parallelism, reencode_per_file_workers, sanitize_filename_segment,
};

#[test]
fn download_archive_paths_are_sanitized_and_stable() {
    let path = download_archive_path("/tmp/manga", "Series / Name", 12.0);

    assert_eq!(
        path.to_string_lossy(),
        "/tmp/manga/Series _ Name/12.tar.zst"
    );
    assert_eq!(download_archive_filename(12.5), "12.5.tar.zst");
}

#[test]
fn comicinfo_xml_normalizes_summary_and_fills_metadata() {
    let chapter = ComicInfoChapterMetadata {
        title: "Chapter <One>",
        number: 4.0,
        count: Some(12),
        date_uploaded: "1704067200",
        page_count: 18,
        age_rating: Some("Teen"),
        source_url: Some("https://example.test/title/chapter-4"),
        language: Some("ja"),
        credits: ComicInfoCredits::default(),
    };

    let xml = build_comicinfo_xml(
        "Series & Co",
        "# Read [official release](https://example.test) & **support**\n\n---\n> - Extra   spaces",
        "Fallback Writer",
        "Action, Mystery",
        &chapter,
    );

    assert!(xml.contains("<Title>Chapter &lt;One&gt;</Title>"));
    assert!(xml.contains("<Series>Series &amp; Co</Series>"));
    assert!(
        xml.contains(
            "<Summary>Read official release (https://example.test) &amp; support\n\nExtra spaces</Summary>"
        )
    );
    assert!(xml.contains("<Writer>Fallback Writer</Writer>"));
    assert!(xml.contains("<LanguageISO>ja</LanguageISO>"));
    assert!(xml.contains("<Year>2024</Year>"));
    assert!(xml.contains("<Month>1</Month>"));
    assert!(xml.contains("<Day>1</Day>"));
    assert!(xml.contains("<Web>https://example.test/title/chapter-4</Web>"));
}

#[test]
fn reencode_worker_split_keeps_at_least_one_worker_per_file() {
    assert_eq!(reencode_file_parallelism(0, 8), 1);
    assert_eq!(reencode_file_parallelism(1, 8), 1);
    assert_eq!(reencode_file_parallelism(3, 8), 2);
    assert_eq!(reencode_file_parallelism(5, 3), 1);

    assert_eq!(reencode_per_file_workers(0, 0), 1);
    assert_eq!(reencode_per_file_workers(2, 8), 4);
    assert_eq!(reencode_per_file_workers(8, 2), 1);
}

#[test]
fn byte_size_parser_handles_blank_zero_and_units() {
    assert_eq!(parse_positive_byte_size("").unwrap(), None);
    assert_eq!(parse_positive_byte_size("0").unwrap(), None);
    assert_eq!(parse_byte_size("0").unwrap(), Some(0));
    assert_eq!(parse_positive_byte_size("1 KiB").unwrap(), Some(1024));
    assert!(parse_byte_size("not-bytes").is_err());
}

#[test]
fn content_type_helpers_ignore_parameters_and_case() {
    assert_eq!(
        primary_content_type(" application/json; charset=utf-8 "),
        "application/json"
    );
    assert!(is_avif_content_type("IMAGE/AVIF; codecs=avif"));
    assert!(is_textual_content_type(
        "application/problem+json; charset=utf-8"
    ));
    assert!(is_textual_content_type("text/html"));
    assert!(!is_textual_content_type("image/png"));
}

#[test]
fn filename_segment_sanitizer_stabilizes_registry_segments() {
    assert_eq!(
        sanitize_filename_segment("1.2.3+build/5", "plugin"),
        "1.2.3-build-5"
    );
    assert_eq!(sanitize_filename_segment("../", "plugin"), "plugin");

    assert!(is_safe_filename_component("source-plugin.wasm"));
    assert!(!is_safe_filename_component("nested/source-plugin.wasm"));
    assert!(!is_safe_filename_component("nested\\source-plugin.wasm"));
    assert!(!is_safe_filename_component("../source-plugin.wasm"));
}
