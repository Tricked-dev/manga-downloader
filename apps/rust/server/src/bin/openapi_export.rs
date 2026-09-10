fn main() {
    let exit_code = match manga_server::export_openapi_to_stdout() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("manga-server openapi export exited with error: {error:#}");
            1
        }
    };

    std::process::exit(exit_code);
}
