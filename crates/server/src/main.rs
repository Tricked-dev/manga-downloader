#![allow(clippy::missing_errors_doc)]

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .enable_metrics_poll_time_histogram()
        .build()
        .expect("tokio runtime should initialize");

    let exit_code = match runtime.block_on(manga_server::run_cli()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("manga-server exited with error: {error:#}");
            1
        }
    };

    std::process::exit(exit_code);
}
