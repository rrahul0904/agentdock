use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::TcpStream;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct HttpRequest {
    pub method: String,
    pub target: String,
    pub body: Vec<u8>,
}

pub fn read_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    let header_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before HTTP headers completed",
            ));
        }

        buffer.extend_from_slice(&chunk[..read]);

        if let Some(end) = find_header_end(&buffer) {
            break end;
        }

        if buffer.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP headers exceed AgentDock limit",
            ));
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = headers.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();

    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    if content_length > MAX_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP body exceeds AgentDock limit",
        ));
    }

    let body_start = header_end + 4;
    let total_needed = body_start + content_length;

    while buffer.len() < total_needed {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before HTTP body completed",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Ok(HttpRequest {
        method,
        target,
        body: buffer[body_start..total_needed].to_vec(),
    })
}

pub fn write_json(
    stream: &mut TcpStream,
    status: u16,
    body: Value,
) -> io::Result<()> {
    let body = serde_json::to_vec_pretty(&body)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;

    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };

    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_header_boundary() {
        let bytes = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\nbody";
        assert_eq!(find_header_end(bytes), Some(33));
    }
}
