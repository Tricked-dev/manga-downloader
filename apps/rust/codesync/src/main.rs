fn main() {
    let exit_code = match codesync::cli::run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("codesync exited with error: {error:#}");
            1
        }
    };

    std::process::exit(exit_code);
}
