//! Minimal direct web-server integration example.
//!
//! Run with an archive path as the first argument, then request `/asset/0`
//! from the listener printed below.

use std::{env, error::Error, path::PathBuf};

use bbf_tokio::AsyncArchive;
use tokio::{
    io::{self, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    runtime::Builder,
};

fn main() -> Result<(), Box<dyn Error>> {
    let archive = env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("archive path is required")?;
    Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(archive))
}

async fn run(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let archive = AsyncArchive::open(path).await?;
    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    println!("listening on http://127.0.0.1:3000/asset/0");
    loop {
        let (stream, _) = listener.accept().await?;
        let archive = archive.clone();
        tokio::spawn(async move {
            if let Err(error) = serve_one(stream, archive).await {
                eprintln!("request failed: {error}");
            }
        });
    }
}

async fn serve_one(mut stream: TcpStream, archive: AsyncArchive) -> Result<(), Box<dyn Error>> {
    let mut request = [0u8; 1024];
    let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut request).await?;
    let asset = archive.asset(0)?;
    let mut body = archive.asset_reader(0).await?;
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                asset.file_size
            )
            .as_bytes(),
        )
        .await?;
    io::copy(&mut body, &mut stream).await?;
    Ok(())
}
