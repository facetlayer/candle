//! Just enough HTTP/1.1 for a localhost dashboard: one request per connection,
//! `Content-Length` bodies only, `Connection: close` on every response.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Longest request line or header line accepted.
const MAX_LINE_BYTES: usize = 8 * 1024;
/// Most header lines accepted.
const MAX_HEADERS: usize = 100;
/// Largest request body accepted; the API only takes small JSON objects.
const MAX_BODY_BYTES: usize = 64 * 1024;
/// A client that stalls longer than this is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// Path without the query string.
    pub path: String,
    /// Header names are lowercased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Response {
            status,
            content_type,
            body: body.into(),
        }
    }

    pub fn json(status: u16, value: &impl serde::Serialize) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        Response::new(status, "application/json", body)
    }

    pub fn error(status: u16, message: &str) -> Self {
        Response::json(status, &serde_json::json!({ "error": message }))
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Why a request could not be parsed; each maps to an error response.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The connection closed or timed out before a full request arrived.
    Disconnected,
    Malformed(&'static str),
    TooLarge,
}

/// Read one line, failing if it exceeds [`MAX_LINE_BYTES`].
fn read_line(reader: &mut impl BufRead) -> Result<String, ParseError> {
    let mut buf = Vec::new();
    let n = reader
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut buf)
        .map_err(|_| ParseError::Disconnected)?;
    if n == 0 {
        return Err(ParseError::Disconnected);
    }
    if !buf.ends_with(b"\n") {
        return Err(if buf.len() > MAX_LINE_BYTES {
            ParseError::TooLarge
        } else {
            ParseError::Disconnected
        });
    }
    while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| ParseError::Malformed("request is not valid UTF-8"))
}

/// Parse a request from a byte stream.
pub fn read_request(reader: impl Read) -> Result<Request, ParseError> {
    let mut reader = BufReader::new(reader);

    let request_line = read_line(&mut reader)?;
    let mut parts = request_line.split(' ');
    let (method, target, version) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v), None) if !m.is_empty() && t.starts_with('/') => (m, t, v),
        _ => return Err(ParseError::Malformed("bad request line")),
    };
    if !version.starts_with("HTTP/1.") {
        return Err(ParseError::Malformed("unsupported HTTP version"));
    }
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();

    let mut headers = Vec::new();
    loop {
        let line = read_line(&mut reader)?;
        if line.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(ParseError::TooLarge);
        }
        let (name, value) = line
            .split_once(':')
            .ok_or(ParseError::Malformed("bad header line"))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }

    let mut request = Request {
        method: method.to_string(),
        path,
        headers,
        body: Vec::new(),
    };

    if request.header("transfer-encoding").is_some() {
        return Err(ParseError::Malformed(
            "chunked request bodies are not supported",
        ));
    }
    let length = match request.header("content-length") {
        Some(v) => v
            .parse::<usize>()
            .map_err(|_| ParseError::Malformed("bad Content-Length"))?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(ParseError::TooLarge);
    }
    request.body = vec![0; length];
    reader
        .read_exact(&mut request.body)
        .map_err(|_| ParseError::Disconnected)?;

    Ok(request)
}

/// Serialize a response with the headers every dashboard response carries.
pub fn encode_response(response: &Response) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        response.status,
        reason_phrase(response.status),
        response.content_type,
        response.body.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(&response.body);
    out
}

/// Serve one request on `stream` with `handler`.
pub fn handle_connection(mut stream: TcpStream, handler: impl FnOnce(&Request) -> Response) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let response = match read_request(&stream) {
        Ok(request) => handler(&request),
        Err(ParseError::Disconnected) => return,
        Err(ParseError::Malformed(why)) => Response::error(400, why),
        Err(ParseError::TooLarge) => Response::error(413, "request too large"),
    };
    let _ = stream.write_all(&encode_response(&response));
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Request, ParseError> {
        read_request(raw.as_bytes())
    }

    #[test]
    fn parses_get_with_query_and_headers() {
        let req =
            parse("GET /api/processes?x=1 HTTP/1.1\r\nHost: localhost:7700\r\nX-A:  b \r\n\r\n")
                .unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/api/processes");
        assert_eq!(req.header("host"), Some("localhost:7700"));
        assert_eq!(req.header("x-a"), Some("b"));
        assert!(req.body.is_empty());
    }

    #[test]
    fn reads_content_length_body() {
        let req = parse("POST /api HTTP/1.1\r\nContent-Length: 7\r\n\r\n{\"a\":1}").unwrap();
        assert_eq!(req.body, b"{\"a\":1}");
    }

    #[test]
    fn short_body_is_a_disconnect() {
        assert_eq!(
            parse("POST /api HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc"),
            Err(ParseError::Disconnected)
        );
    }

    #[test]
    fn rejects_malformed_and_oversized_requests() {
        assert!(matches!(
            parse("GARBAGE\r\n\r\n"),
            Err(ParseError::Malformed(_))
        ));
        assert!(matches!(
            parse("GET http://evil/ HTTP/1.1\r\n\r\n"),
            Err(ParseError::Malformed(_))
        ));
        assert!(matches!(
            parse("GET / HTTP/1.1\r\nNoColon\r\n\r\n"),
            Err(ParseError::Malformed(_))
        ));
        assert!(matches!(
            parse("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Err(ParseError::Malformed(_))
        ));
        let big = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        assert_eq!(parse(&big), Err(ParseError::TooLarge));
        let long_line = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_LINE_BYTES + 10));
        assert_eq!(parse(&long_line), Err(ParseError::TooLarge));
    }

    #[test]
    fn empty_stream_is_a_disconnect() {
        assert_eq!(parse(""), Err(ParseError::Disconnected));
    }

    #[test]
    fn encodes_status_headers_and_body() {
        let bytes = encode_response(&Response::new(404, "text/plain", "nope"));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(text.contains("Content-Length: 4\r\n"));
        assert!(text.contains("Connection: close\r\n"));
        assert!(text.ends_with("\r\n\r\nnope"));
    }
}
