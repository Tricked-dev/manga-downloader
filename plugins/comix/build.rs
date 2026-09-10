fn main() {
    println!("cargo::rerun-if-changed=src");
    println!("cargo::rerun-if-changed=../../crates/plugin-host/wit/manga-source.wit");
}
