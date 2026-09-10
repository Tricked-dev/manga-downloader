fn main() {
    shadow_rs::ShadowBuilder::builder()
        .build()
        .expect("shadow-rs build should succeed");
    embed_client_packages();
}

fn embed_client_packages() {
    let mut generated = String::new();
    for (name, variable) in [
        ("AIDOKU", "MANGA_EMBED_AIDOKU_PACKAGE"),
        ("TACHIYOMI", "MANGA_EMBED_TACHIYOMI_PACKAGE"),
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
        let value = if let Some(path) = std::env::var_os(variable) {
            let path = std::fs::canonicalize(path)
                .unwrap_or_else(|error| panic!("cannot open {variable}: {error}"));
            let metadata = std::fs::metadata(&path).expect("client package metadata is readable");
            assert!(
                metadata.is_file() && metadata.len() > 0,
                "{variable} must name a nonempty package"
            );
            println!("cargo:rerun-if-changed={}", path.display());
            format!("Some(include_bytes!({path:?}))")
        } else {
            "None".into()
        };
        generated.push_str(&format!("pub const {name}: Option<&[u8]> = {value};\n"));
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    std::fs::write(output.join("client_packages.rs"), generated)
        .expect("embedded client declarations can be written");
}
