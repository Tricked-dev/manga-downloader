fn main() {
    println!("cargo::rerun-if-changed=src");
    println!("cargo::rerun-if-changed=../../libs/rust/plugin-host/wit/manga-source.wit");
}
