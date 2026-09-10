use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use tempfile::TempDir;

const DIRECTORY_COUNT: usize = 24;
const FILES_PER_DIRECTORY: usize = 12;
const FILE_BYTES: usize = 1024;

fn write_sample_tree() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp dir is created");
    let payload = vec![b'x'; FILE_BYTES];

    for directory_index in 0..DIRECTORY_COUNT {
        let nested = temp_dir.path().join(format!("series-{directory_index:03}"));
        std::fs::create_dir_all(&nested).expect("sample directory is created");

        for file_index in 0..FILES_PER_DIRECTORY {
            let extension = if file_index % 3 == 0 { "avif" } else { "jpg" };
            let path = nested.join(format!("chapter-{file_index:03}.{extension}"));
            std::fs::write(path, &payload).expect("sample file is written");
        }
    }

    temp_dir
}

fn bench_directory_size(c: &mut Criterion) {
    let temp_dir = write_sample_tree();
    c.bench_function("directory_size_bytes", |b| {
        b.iter(|| {
            backend_fs::directory_size_bytes(black_box(temp_dir.path()))
                .expect("directory size is scanned");
        });
    });
}

fn bench_suffix_collection(c: &mut Criterion) {
    c.bench_function("collect_files_with_suffix", |b| {
        b.iter_batched(
            write_sample_tree,
            |temp_dir| {
                backend_fs::collect_files_with_suffix(
                    black_box(temp_dir.path()),
                    black_box(".avif"),
                )
                .expect("suffix collection succeeds");
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_directory_size, bench_suffix_collection);
criterion_main!(benches);
