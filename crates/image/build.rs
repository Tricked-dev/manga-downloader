fn main() {
    println!("cargo:rerun-if-changed=src/avif.c");
    let mut config = pkg_config::Config::new();
    config.atleast_version("1.0").cargo_metadata(false);
    let library = config
        .probe("libavif")
        .expect("libavif with libaom is required for lossless AVIF; enter the project development environment");
    let mut bridge = cc::Build::new();
    bridge.file("src/avif.c").opt_level(2);
    for include in library.include_paths {
        bridge.include(include);
    }
    bridge.compile("manga_avif");
    // GNU ld resolves libraries left to right. Emit libavif after the static
    // bridge that references it, so --as-needed does not discard it first.
    config
        .cargo_metadata(true)
        .probe("libavif")
        .expect("libavif was already located above");
}
