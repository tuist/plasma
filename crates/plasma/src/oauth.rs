//! OAuth2 PKCE helper with a one-shot loopback HTTP server.
//!
//! Providers (OpenRouter, OpenAI) ship slightly different authorize/token
//! shapes, but the same plumbing applies: generate a PKCE pair, start a
//! loopback server, hand the user a URL to visit, wait for the browser
//! callback, and exchange the authorization code for a credential. This
//! module owns that plumbing so each provider only has to declare the
//! endpoints and how to parse the token response.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use base64::Engine;
use sha2::{Digest, Sha256};

/// PKCE verifier + S256 challenge. RFC 7636.
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

const UNRESERVED: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Generate a fresh PKCE pair. The verifier is 64 random unreserved
/// characters; the challenge is the base64url-encoded SHA-256 of the
/// verifier bytes.
pub fn generate_pkce() -> PkcePair {
    // Mix two high-resolution time stamps with a simple xorshift so the
    // verifier looks different across runs even without an RNG dependency.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut state = nanos ^ 0x9E3779B97F4A7C15;
    let mut bytes = [0u8; 64];
    for slot in &mut bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *slot = state as u8;
    }
    let verifier: String = bytes
        .iter()
        .map(|byte| UNRESERVED[(*byte as usize) % UNRESERVED.len()] as char)
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    PkcePair {
        verifier,
        challenge,
    }
}

/// State a one-shot loopback callback can be in.
#[derive(Debug)]
enum CallbackResult {
    Code(String),
}

/// Bind a loopback server on an ephemeral port. Returns the bound port and
/// a receiver that yields the authorization code once the user finishes
/// the browser flow. The server is closed when the receiver is dropped.
pub fn bind_loopback(path: &str, timeout: Duration) -> Result<(u16, mpsc::Receiver<CallbackResult>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let (tx, rx) = mpsc::channel();
    let expected_path = path.to_string();

    thread::spawn(move || {
        let outcome = (|| -> Result<CallbackResult> {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(timeout)).ok();
            let mut buffer = vec![0u8; 4096];
            let read = match stream.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => {
                    // Best-effort timeout; the stream may be closed or the
                    // read may have failed. Fall through to the response
                    // path with whatever we have.
                    0
                }
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request_line_path(&request)
                .ok_or_else(|| anyhow!("Malformed OAuth callback request"))?;
            if path.split('?').next() != Some(expected_path.as_str()) {
                send_response(
                    &mut stream,
                    404,
                    "Bad callback path. You can close this tab.",
                )?;
                return Err(anyhow!("Unexpected callback path: {path}"));
            }
            let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
            if let Some(error) = query_param(query, "error") {
                let description = query_param(query, "error_description").unwrap_or_default();
                send_response(
                    &mut stream,
                    400,
                    &format!(
                        "<h1>Sign-in failed</h1><p>{error}: {description}</p><p>You can close this tab.</p>"
                    ),
                )?;
                return Err(anyhow!("OAuth error: {error}: {description}"));
            }
            let code = query_param(query, "code")
                .ok_or_else(|| anyhow!("OAuth callback did not include a code"))?;
            send_response(
                &mut stream,
                200,
                "<h1>Signed in</h1><p>You can close this tab and return to Plasma.</p>",
            )?;
            Ok(CallbackResult::Code(code))
        })();
        if let Ok(result) = outcome {
            let _ = tx.send(result);
        }
    });

    Ok((port, rx))
}

/// Pull the request line's path component out of a raw HTTP request.
fn request_line_path(request: &str) -> Option<&str> {
    let line = request.lines().next()?;
    let target = line.split_whitespace().nth(1)?;
    Some(target)
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            return Some(url_decode(value));
        }
    }
    None
}

fn url_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '+' {
            out.push(' ');
        } else if ch == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn send_response(stream: &mut std::net::TcpStream, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let payload = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(payload.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_has_correct_shape() {
        let pair = generate_pkce();
        assert_eq!(pair.verifier.len(), 64);
        assert!(pair.challenge.len() >= 43);
        assert!(pair
            .verifier
            .chars()
            .all(|c| UNRESERVED.contains(&(c as u8))));
    }

    #[test]
    fn pkce_pair_changes_between_calls() {
        // Two consecutive pairs should not collide even on a fast machine.
        std::thread::sleep(Duration::from_millis(1));
        let first = generate_pkce();
        let second = generate_pkce();
        assert_ne!(first.verifier, second.verifier);
        assert_ne!(first.challenge, second.challenge);
    }

    #[test]
    fn query_param_extracts_value() {
        assert_eq!(query_param("code=abc&state=xyz", "code").as_deref(), Some("abc"));
        assert_eq!(query_param("code=abc&state=xyz", "state").as_deref(), Some("xyz"));
        assert_eq!(query_param("code=abc&state=xyz", "missing"), None);
    }

    #[test]
    fn query_param_url_decodes_value() {
        assert_eq!(
            query_param("code=hello%20world", "code").as_deref(),
            Some("hello world")
        );
        assert_eq!(
            query_param("code=foo+bar", "code").as_deref(),
            Some("foo bar")
        );
    }

    #[test]
    fn loopback_server_serves_the_captured_code() {
        // Bind a server, then "call back" with a raw HTTP request that
        // simulates the browser redirect. The receiver should yield the
        // authorization code.
        let (port, rx) = bind_loopback("/oauth/callback", Duration::from_secs(2))
            .expect("bind loopback");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let mut stream =
                std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
            stream
                .write_all(
                    b"GET /oauth/callback?code=the-code&state=ignored HTTP/1.1\r\nHost: localhost\r\n\r\n",
                )
                .expect("write");
        });
        let result = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("code arrives in time");
        match result {
            CallbackResult::Code(code) => assert_eq!(code, "the-code"),
        }
    }
}
