use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyTarget {
    Forward(SocketAddr),
    Unavailable,
    Unknown,
}

pub trait TargetResolver: Send + Sync + 'static {
    fn resolve(&self, hostname: &str) -> Result<ProxyTarget, String>;
}

pub fn serve(bind: SocketAddr, resolver: Arc<dyn TargetResolver>) -> io::Result<()> {
    let listener = TcpListener::bind(bind)?;
    for client in listener.incoming() {
        match client {
            Ok(client) => {
                let resolver = Arc::clone(&resolver);
                thread::spawn(move || {
                    if let Err(error) = handle_client(client, resolver.as_ref()) {
                        eprintln!("AgentDock proxy connection error: {error}");
                    }
                });
            }
            Err(error) => eprintln!("AgentDock proxy accept error: {error}"),
        }
    }
    Ok(())
}

fn handle_client(mut client: TcpStream, resolver: &dyn TargetResolver) -> io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(10)))?;
    let request = read_initial_request(&mut client)?;

    let Some(hostname) = hostname_from_request(&request) else {
        return write_error(&mut client, 400, "Bad Request", "missing Host header");
    };

    let target = resolver
        .resolve(&hostname)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;

    let target = match target {
        ProxyTarget::Forward(target) => target,
        ProxyTarget::Unavailable => {
            return write_error(
                &mut client,
                503,
                "Service Unavailable",
                "project has no active HTTP service",
            )
        }
        ProxyTarget::Unknown => {
            return write_error(&mut client, 404, "Not Found", "unknown AgentDock hostname")
        }
    };

    let mut upstream = TcpStream::connect_timeout(&target, Duration::from_secs(3))?;
    upstream.set_read_timeout(Some(Duration::from_secs(300)))?;
    upstream.set_write_timeout(Some(Duration::from_secs(30)))?;
    upstream.write_all(&request)?;

    let mut client_reader = client.try_clone()?;
    let mut upstream_writer = upstream.try_clone()?;

    thread::spawn(move || {
        let _ = io::copy(&mut client_reader, &mut upstream_writer);
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });

    let _ = io::copy(&mut upstream, &mut client);
    let _ = client.shutdown(Shutdown::Write);
    Ok(())
}

fn read_initial_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut request = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);

        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }

        if request.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP request headers exceeded AgentDock limit",
            ));
        }
    }

    if request.is_empty() {
        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty request"))
    } else {
        Ok(request)
    }
}

pub fn hostname_from_request(request: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(request);
    for line in text.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("host") {
            return normalize_host(value);
        }
    }
    None
}

pub fn normalize_host(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let host = if value.starts_with('[') {
        let end = value.find(']')?;
        &value[1..end]
    } else {
        value.split_once(':').map(|(host, _)| host).unwrap_or(value)
    };

    let normalized = host.trim().trim_end_matches('.').to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

fn write_error(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    message: &str,
) -> io::Result<()> {
    let body = format!("AgentDock: {message}\n");
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_localhost_hostname_with_port() {
        let request = b"GET / HTTP/1.1\r\nHost: StoreFront.localhost:7777\r\n\r\n";
        assert_eq!(
            hostname_from_request(request).as_deref(),
            Some("storefront.localhost")
        );
    }

    #[test]
    fn normalizes_trailing_dot() {
        assert_eq!(
            normalize_host("api.localhost.").as_deref(),
            Some("api.localhost")
        );
    }
}
