use std::path::Path;

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("test directory should be created");
    }
    std::fs::write(path, bytes).expect("test file should be written");
}

#[test]
fn archive_build_inspect_and_extract_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let page_10 = temp.path().join("page10.jpg");
    let page_2 = temp.path().join("nested/page2.png");
    let archive_path = temp.path().join("chapter.tar.zst");
    write_file(&page_10, b"page ten");
    write_file(&page_2, b"page two");

    backend_image::build_zstd_folder_from_paths(
        &[
            ("page10.jpg".to_string(), page_10),
            ("nested/page2.png".to_string(), page_2),
        ],
        &archive_path,
        Some(
            "<ComicInfo><Title>Chapter 2</Title><Series>Series</Series><PageCount>2</PageCount></ComicInfo>",
        ),
        Some(("cover.jpg".to_string(), b"cover bytes".to_vec())),
    )
    .expect("archive should be built");

    let info = backend_image::inspect_archive(&archive_path).expect("archive should inspect");

    assert_eq!(
        info.pages
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["nested/page2.png", "page10.jpg"]
    );
    assert_eq!(info.cover_entry.as_deref(), Some("cover.jpg"));
    assert_eq!(info.comic_info.title.as_deref(), Some("Chapter 2"));
    assert_eq!(info.comic_info.page_count, Some(2));
    assert_ne!(info.pages[0].file_position, info.pages[1].file_position);

    let first_page = backend_image::extract_page(&archive_path, 0).expect("page should extract");
    assert_eq!(first_page.content_type, "image/png");
    assert_eq!(first_page.bytes, b"page two");

    let positioned = backend_image::extract_positioned_images(
        &archive_path,
        &[info.pages[1].file_position, info.pages[0].file_position],
    )
    .expect("positioned pages should extract");
    assert_eq!(positioned[0].bytes, b"page ten");
    assert_eq!(positioned[1].bytes, b"page two");

    let cover = backend_image::extract_cover(&archive_path)
        .expect("cover should extract")
        .expect("cover should exist");
    assert_eq!(cover.content_type, "image/jpeg");
    assert_eq!(cover.bytes, b"cover bytes");
}

#[test]
fn archive_cover_fallback_uses_first_natural_page() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let page_10 = temp.path().join("page10.jpg");
    let page_2 = temp.path().join("page2.jpg");
    let archive_path = temp.path().join("chapter.tar.zst");
    write_file(&page_10, b"page ten");
    write_file(&page_2, b"page two");

    backend_image::build_zstd_folder_from_paths(
        &[
            ("page10.jpg".to_string(), page_10),
            ("page2.jpg".to_string(), page_2),
        ],
        &archive_path,
        None,
        None,
    )
    .expect("archive should be built");

    let fallback = backend_image::extract_cover_or_first_page(&archive_path)
        .expect("fallback extraction should succeed")
        .expect("first page should exist");

    assert_eq!(fallback.content_type, "image/jpeg");
    assert_eq!(fallback.bytes, b"page two");
}

#[test]
fn content_type_for_known_image_extensions_is_case_insensitive() {
    assert_eq!(backend_image::content_type_for("page.JPG"), "image/jpeg");
    assert_eq!(backend_image::content_type_for("page.png"), "image/png");
    assert_eq!(backend_image::content_type_for("page.avif"), "image/avif");
    assert_eq!(
        backend_image::content_type_for("page.unknown"),
        "application/octet-stream"
    );
}

#[test]
fn detect_supported_image_format_rejects_html_bytes() {
    let error = backend_image::detect_supported_image_format(b"<html>not an image</html>")
        .expect_err("html bytes should not be detected as an image");

    assert!(
        format!("{error:#}").contains("image format could not be determined"),
        "unexpected error: {error:#}"
    );
}

#[test]
fn staged_avif_conversion_reports_invalid_page_name() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let page_path = temp.path().join("0001.jpg");
    write_file(&page_path, b"<html>not an image</html>");

    let error = backend_image::convert_staged_pages_to_avif_with_workers_cancelable_progress(
        vec![("0001.jpg".to_string(), page_path)],
        80,
        1,
        None,
        None,
    )
    .expect_err("invalid staged image should fail conversion");
    let message = format!("{error:#}");

    assert!(
        message.contains("failed to convert staged page 0001.jpg"),
        "unexpected error: {message}"
    );
    assert!(
        message.contains("failed to decode image bytes"),
        "unexpected error: {message}"
    );
}
