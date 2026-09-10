use std::path::PathBuf;

fn write_file(path: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("test directory should be created");
    }
    std::fs::write(path, bytes).expect("test file should be written");
}

#[test]
fn directory_size_bytes_recurses_and_ignores_missing_roots() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    write_file(&temp.path().join("chapter/page-1.avif"), b"12345");
    write_file(&temp.path().join("chapter/nested/page-2.avif"), b"123");

    assert_eq!(
        backend_fs::directory_size_bytes(temp.path()).expect("directory size should be scanned"),
        8
    );
    assert_eq!(
        backend_fs::directory_size_bytes(&temp.path().join("missing"))
            .expect("missing roots should be treated as empty"),
        0
    );
}

#[test]
fn collect_files_with_suffix_is_recursive_case_insensitive_and_sorted() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    write_file(&temp.path().join("b/second.TAR.ZST"), b"second");
    write_file(&temp.path().join("a/first.tar.zst"), b"first");
    write_file(&temp.path().join("a/skip.txt"), b"skip");

    let files = backend_fs::collect_files_with_suffix(temp.path(), ".tar.zst")
        .expect("matching files should be collected");
    let relative_files = files
        .iter()
        .map(|path| {
            path.strip_prefix(temp.path())
                .expect("file should be inside tempdir")
                .to_path_buf()
        })
        .collect::<Vec<PathBuf>>();

    assert_eq!(
        relative_files,
        vec![
            PathBuf::from("a/first.tar.zst"),
            PathBuf::from("b/second.TAR.ZST"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn async_write_read_and_list_paths_sorted_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let b_path = temp.path().join("b.txt");
    let a_path = temp.path().join("a.txt");

    backend_fs::write_bytes(&b_path, b"b")
        .await
        .expect("b file should be written");
    backend_fs::write_bytes(&a_path, b"a")
        .await
        .expect("a file should be written");

    let entries = backend_fs::list_paths_sorted(temp.path())
        .await
        .expect("directory should be listed");
    let names = entries
        .iter()
        .map(|path| {
            path.file_name()
                .expect("entry should have a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["a.txt", "b.txt"]);
    assert_eq!(
        backend_fs::read_bytes(&a_path)
            .await
            .expect("a file should be read"),
        b"a"
    );
}
