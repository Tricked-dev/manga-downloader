use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as FmtWrite;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use url::Url;

const DEFAULT_ADDR: &str = "127.0.0.1:8888";
const HEADER_LIMIT: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let listener = TcpListener::bind(&config.addr)
        .await
        .with_context(|| format!("failed to bind {}", config.addr))?;

    eprintln!("manga http proxy listening on {}", config.addr);

    loop {
        let (stream, peer_addr) = listener.accept().await.context("failed to accept client")?;
        let config = config.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, config).await {
                eprintln!("proxy connection from {peer_addr} failed: {err:#}");
            }
        });
    }
}

#[derive(Clone, Debug)]
struct Config {
    addr: String,
    connect_ports: BTreeSet<u16>,
}

impl Config {
    fn load() -> Result<Self> {
        let addr = env::var("MANGA_HTTP_PROXY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
        let connect_ports = env::var("MANGA_HTTP_PROXY_CONNECT_PORTS")
            .map(|value| parse_connect_ports(&value))
            .unwrap_or_else(|_| Ok(BTreeSet::from([443])))?;

        Ok(Self {
            addr,
            connect_ports,
        })
    }
}

async fn handle_client(mut client: TcpStream, config: Config) -> Result<()> {
    let request = match read_request(&mut client).await {
        Ok(request) => request,
        Err(err) => {
            write_error(&mut client, 400, "Bad Request").await?;
            return Err(err);
        }
    };

    if request.head.method.eq_ignore_ascii_case("CONNECT") {
        tunnel_connect(&mut client, &request.head, &config).await
    } else {
        proxy_http_request(&mut client, request).await
    }
}

async fn tunnel_connect(client: &mut TcpStream, head: &RequestHead, config: &Config) -> Result<()> {
    let target = parse_host_port(&head.target, 443)?;
    if !config.connect_ports.contains(&target.port) {
        write_error(client, 403, "Forbidden").await?;
        bail!("connect port {} is not allowed", target.port);
    }

    let mut upstream = TcpStream::connect(target.socket_addr())
        .await
        .with_context(|| format!("failed to connect to {}", target.display()))?;

    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .context("failed to write connect response")?;

    tokio::io::copy_bidirectional(client, &mut upstream)
        .await
        .context("failed while tunneling data")?;
    Ok(())
}

async fn proxy_http_request(client: &mut TcpStream, request: Request) -> Result<()> {
    let target = HttpTarget::parse(&request.head.target)?;
    let mut upstream = TcpStream::connect(target.socket_addr())
        .await
        .with_context(|| format!("failed to connect to {}", target.authority()))?;

    let bytes = rewrite_http_request(&request, &target)?;
    upstream
        .write_all(&bytes)
        .await
        .context("failed to write upstream request")?;

    tokio::io::copy(&mut upstream, client)
        .await
        .context("failed to copy upstream response")?;
    Ok(())
}

async fn read_request(client: &mut TcpStream) -> Result<Request> {
    let mut buffer = Vec::with_capacity(4096);

    loop {
        let mut chunk = [0_u8; 4096];
        let read = timeout(READ_TIMEOUT, client.read(&mut chunk))
            .await
            .context("timed out reading request headers")?
            .context("failed to read request headers")?;

        if read == 0 {
            bail!("client closed before sending request headers");
        }

        buffer.extend_from_slice(&chunk[..read]);

        if buffer.len() > HEADER_LIMIT {
            bail!("request headers exceeded {HEADER_LIMIT} bytes");
        }

        if let Some(header_end) = find_header_end(&buffer) {
            let body = buffer.split_off(header_end + 4);
            let head = parse_request_head(&buffer)?;
            return Ok(Request { head, body });
        }
    }
}

fn parse_request_head(bytes: &[u8]) -> Result<RequestHead> {
    let text = std::str::from_utf8(bytes).context("request headers were not utf-8")?;
    let mut lines = text.split("\r\n");
    let first_line = lines
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = first_line.split_ascii_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("missing request method"))?
        .to_owned();
    let target = parts
        .next()
        .ok_or_else(|| anyhow!("missing request target"))?
        .to_owned();
    let version = parts
        .next()
        .ok_or_else(|| anyhow!("missing http version"))?
        .to_owned();

    if parts.next().is_some() {
        bail!("request line had too many fields");
    }

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow!("malformed header line"))?;
        headers.push(Header {
            name: name.trim().to_owned(),
            value: value.trim().to_owned(),
        });
    }

    Ok(RequestHead {
        method,
        target,
        version,
        headers,
    })
}

fn rewrite_http_request(request: &Request, target: &HttpTarget) -> Result<Vec<u8>> {
    let mut rewritten = String::new();
    writeln!(
        rewritten,
        "{} {} {}\r",
        request.head.method, target.path_and_query, request.head.version
    )
    .context("failed to rewrite request line")?;

    let mut has_host = false;
    for header in &request.head.headers {
        if header.name.eq_ignore_ascii_case("host") {
            has_host = true;
        }

        if header.name.eq_ignore_ascii_case("connection")
            || header.name.eq_ignore_ascii_case("proxy-connection")
            || header.name.eq_ignore_ascii_case("proxy-authorization")
        {
            continue;
        }

        writeln!(rewritten, "{}: {}\r", header.name, header.value)
            .context("failed to rewrite request header")?;
    }

    if !has_host {
        writeln!(rewritten, "Host: {}\r", target.authority())
            .context("failed to write host header")?;
    }
    rewritten.push_str("Connection: close\r\n\r\n");

    let mut bytes = rewritten.into_bytes();
    bytes.extend_from_slice(&request.body);
    Ok(bytes)
}

async fn write_error(client: &mut TcpStream, status: u16, reason: &str) -> Result<()> {
    let response =
        format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    client
        .write_all(response.as_bytes())
        .await
        .context("failed to write error response")
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_connect_ports(value: &str) -> Result<BTreeSet<u16>> {
    let mut ports = BTreeSet::new();
    for raw_port in value.split(',') {
        let port = raw_port
            .trim()
            .parse::<u16>()
            .with_context(|| format!("invalid connect port {raw_port:?}"))?;
        if port == 0 {
            bail!("connect port cannot be 0");
        }
        ports.insert(port);
    }

    if ports.is_empty() {
        bail!("at least one connect port must be configured");
    }

    Ok(ports)
}

#[derive(Debug)]
struct Request {
    head: RequestHead,
    body: Vec<u8>,
}

#[derive(Debug)]
struct RequestHead {
    method: String,
    target: String,
    version: String,
    headers: Vec<Header>,
}

#[derive(Debug)]
struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Eq, PartialEq)]
struct HostPort {
    host: String,
    port: u16,
}

impl HostPort {
    fn display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn socket_addr(&self) -> String {
        socket_addr(&self.host, self.port)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct HttpTarget {
    host: String,
    port: u16,
    path_and_query: String,
}

impl HttpTarget {
    fn parse(target: &str) -> Result<Self> {
        let url = Url::parse(target).with_context(|| format!("invalid proxy url {target:?}"))?;
        if url.scheme() != "http" {
            bail!("only absolute http urls are supported outside CONNECT");
        }

        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("http proxy url is missing a host"))?
            .to_owned();
        let port = url
            .port_or_known_default()
            .ok_or_else(|| anyhow!("http proxy url is missing a port"))?;
        let path_and_query = url[url::Position::BeforePath..].to_owned();

        Ok(Self {
            host,
            port,
            path_and_query,
        })
    }

    fn authority(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn socket_addr(&self) -> String {
        socket_addr(&self.host, self.port)
    }
}

fn parse_host_port(target: &str, default_port: u16) -> Result<HostPort> {
    if let Some(rest) = target.strip_prefix('[') {
        let (host, after_host) = rest
            .split_once(']')
            .ok_or_else(|| anyhow!("invalid bracketed host in {target:?}"))?;
        let port = if let Some(port) = after_host.strip_prefix(':') {
            parse_port(port)?
        } else {
            default_port
        };
        return Ok(HostPort {
            host: host.to_owned(),
            port,
        });
    }

    let (host, port) = if let Some((host, port)) = target.rsplit_once(':') {
        (host, parse_port(port)?)
    } else {
        (target, default_port)
    };

    if host.is_empty() {
        bail!("target host cannot be empty");
    }

    Ok(HostPort {
        host: host.to_owned(),
        port,
    })
}

fn parse_port(port: &str) -> Result<u16> {
    let parsed = port
        .parse::<u16>()
        .with_context(|| format!("invalid port {port:?}"))?;
    if parsed == 0 {
        bail!("port cannot be 0");
    }
    Ok(parsed)
}

fn socket_addr(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connect_host_port() {
        assert_eq!(
            parse_host_port("example.com:443", 443).expect("host port should parse"),
            HostPort {
                host: "example.com".to_owned(),
                port: 443,
            }
        );
        assert_eq!(
            parse_host_port("[::1]:8443", 443).expect("ipv6 host port should parse"),
            HostPort {
                host: "::1".to_owned(),
                port: 8443,
            }
        );
    }

    #[test]
    fn parses_absolute_http_target() {
        assert_eq!(
            HttpTarget::parse("http://example.com:8080/path?q=1")
                .expect("http target should parse"),
            HttpTarget {
                host: "example.com".to_owned(),
                port: 8080,
                path_and_query: "/path?q=1".to_owned(),
            }
        );
    }

    #[test]
    fn rewrites_proxy_request_to_origin_form() {
        let request = Request {
            head: RequestHead {
                method: "GET".to_owned(),
                target: "http://example.com/path?q=1".to_owned(),
                version: "HTTP/1.1".to_owned(),
                headers: vec![
                    Header {
                        name: "Host".to_owned(),
                        value: "example.com".to_owned(),
                    },
                    Header {
                        name: "Proxy-Connection".to_owned(),
                        value: "keep-alive".to_owned(),
                    },
                ],
            },
            body: Vec::new(),
        };
        let target = HttpTarget::parse(&request.head.target).expect("target should parse");
        let rewritten = rewrite_http_request(&request, &target).expect("request should rewrite");
        let text = String::from_utf8(rewritten).expect("request should be utf-8");

        assert_eq!(
            text,
            "GET /path?q=1 HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n"
        );
    }
}
