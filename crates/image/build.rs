fn main() {
    println!("cargo:rerun-if-changed=src/avif.c");
    let library = pkg_config::Config::new()
        .atleast_version("1.0")
        .probe("libavif")
        .expect("libavif with libaom is required for lossless AVIF; enter the project development environment");
    let mut bridge = cc::Build::new();
    bridge.file("src/avif.c").opt_level(2);
    for include in library.include_paths {
        bridge.include(include);
    }
    bridge.compile("manga_avif");
}
