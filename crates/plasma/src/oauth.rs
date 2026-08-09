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
pub enum CallbackResult {
    Code(String),
}

/// State of the OAuth callback page. The loopback server renders the
/// success or error variant once the user finishes the browser flow.
#[derive(Debug, Clone, Copy)]
pub enum CallbackPage {
    Success,
    Error,
}

/// Build the HTML body for the OAuth callback page. The page is styled
/// with Noora, Tuist's design system. Noora ships a compiled CSS file at
/// `noora/priv/static/noora.css` in the tuist/tuist repo; we load it
/// from jsdelivr so the loopback server doesn't have to bundle assets.
pub fn callback_page(state: CallbackPage) -> String {
    let noora_css = "https://cdn.jsdelivr.net/gh/tuist/tuist@main/noora/priv/static/noora.css";
    let inter = "https://rsms.me/inter/inter.css";

    let (badge, title, body, auto_close) = match state {
        CallbackPage::Success => (
            "success",
            "Signed in",
            "You can close this tab and return to Plasma.",
            true,
        ),
        CallbackPage::Error => (
            "error",
            "Sign-in failed",
            "Something went wrong while completing the sign-in flow. \
             You can close this tab and try again from Plasma.",
            false,
        ),
    };

    let icon = match state {
        CallbackPage::Success => r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>"#,
        CallbackPage::Error => r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>"#,
    };

    let auto_close_script = if auto_close {
        // Try to close the tab after a short pause. The browser may
        // refuse if the tab wasn't opened by a script, in which case the
        // "You can close this tab" copy is still visible.
        r#"<script>
            setTimeout(function() {
                try { window.close(); } catch (_) {}
            }, 2500);
        </script>"#
    } else {
        ""
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Plasma</title>
<link rel="stylesheet" href="{inter}">
<link rel="stylesheet" href="{noora_css}">
<style>
  :root {{
    color-scheme: dark;
  }}
  html, body {{
    margin: 0;
    padding: 0;
    min-height: 100vh;
    background: var(--noora-surface-background-primary);
    color: var(--noora-surface-label-primary);
    font-family: var(--noora-font-body);
    -webkit-font-smoothing: antialiased;
  }}
  body {{
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--noora-spacing-9);
  }}
  .callback-card {{
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--noora-spacing-6);
    max-width: 420px;
    width: 100%;
    padding: var(--noora-spacing-10) var(--noora-spacing-9);
    background: var(--noora-surface-background-secondary);
    border-radius: var(--noora-radius-8);
    box-shadow: var(--noora-border-medium);
    text-align: center;
  }}
  .callback-icon {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 64px;
    height: 64px;
    border-radius: var(--noora-radius-99);
    padding: var(--noora-spacing-4);
  }}
  .callback-icon svg {{
    width: 32px;
    height: 32px;
  }}
  .callback-icon[data-badge="success"] {{
    background: var(--noora-icon-success-background);
    color: var(--noora-icon-success-label);
  }}
  .callback-icon[data-badge="error"] {{
    background: var(--noora-icon-destructive-background);
    color: var(--noora-icon-destructive-label);
  }}
  .callback-icon[data-badge="success"] svg {{
    animation: callback-icon-pop 400ms var(--ease-out-back, cubic-bezier(0.34, 1.56, 0.64, 1)) both;
  }}
  @keyframes callback-icon-pop {{
    0%   {{ transform: scale(0.4); opacity: 0; }}
    100% {{ transform: scale(1);   opacity: 1; }}
  }}
  h1 {{
    margin: 0;
    font: var(--noora-font-weight-semibold) var(--noora-font-heading-large);
    color: var(--noora-surface-label-primary);
    letter-spacing: -0.01em;
  }}
  p {{
    margin: 0;
    font: var(--noora-font-weight-regular) var(--noora-font-body-medium);
    color: var(--noora-surface-label-secondary);
    line-height: 1.5;
  }}
  .callback-footer {{
    margin-top: var(--noora-spacing-2);
    font: var(--noora-font-weight-medium) var(--noora-font-body-xsmall);
    color: var(--noora-surface-label-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }}
</style>
</head>
<body>
  <main class="callback-card" data-badge="{badge}">
    <div class="callback-icon" data-badge="{badge}" aria-hidden="true">{icon}</div>
    <h1>{title}</h1>
    <p>{body}</p>
    <div class="callback-footer">Plasma</div>
  </main>
  {auto_close_script}
</body>
</html>"#
    )
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
                send_response(&mut stream, 404, &callback_page(CallbackPage::Error))?;
                return Err(anyhow!("Unexpected callback path: {path}"));
            }
            let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
            if let Some(error) = query_param(query, "error") {
                let description = query_param(query, "error_description").unwrap_or_default();
                send_response(&mut stream, 400, &callback_page(CallbackPage::Error))?;
                return Err(anyhow!("OAuth error: {error}: {description}"));
            }
            let code = query_param(query, "code")
                .ok_or_else(|| anyhow!("OAuth callback did not include a code"))?;
            send_response(&mut stream, 200, &callback_page(CallbackPage::Success))?;
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
        body.as_bytes().len()
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

    #[test]
    fn callback_page_success_loads_noora_and_inter() {
        let html = callback_page(CallbackPage::Success);
        assert!(html.contains("Signed in"));
        assert!(html.contains("noora.css"), "success page should link Noora's CSS");
        assert!(html.contains("rsms.me/inter"), "success page should load Inter font");
        assert!(html.contains("data-badge=\"success\""));
        // The success page should auto-close the tab; the error page should not.
        assert!(html.contains("window.close()"));
    }

    #[test]
    fn callback_page_error_omits_auto_close() {
        let html = callback_page(CallbackPage::Error);
        assert!(html.contains("Sign-in failed"));
        assert!(html.contains("data-badge=\"error\""));
        assert!(!html.contains("window.close()"), "error page should not auto-close");
    }

    #[test]
    fn callback_page_uses_noora_design_tokens() {
        // The page should style itself with Noora's design tokens so it
        // matches the rest of the Tuist product surface.
        let html = callback_page(CallbackPage::Success);
        for token in [
            "--noora-surface-background-primary",
            "--noora-surface-background-secondary",
            "--noora-surface-label-primary",
            "--noora-surface-label-secondary",
            "--noora-font-body",
            "--noora-radius-8",
            "--noora-spacing-9",
        ] {
            assert!(
                html.contains(token),
                "callback page should reference {token}"
            );
        }
    }

    #[test]
    fn send_response_uses_byte_length_not_char_length() {
        // Regression: the body now contains multi-byte UTF-8 (em dash,
        // quotes, box-drawing characters). The Content-Length header
        // must report the byte count or the browser will truncate the
        // page mid-tag.
        use std::io::Write;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            send_response(&mut stream, 200, "hello \u{2014} world").expect("send");
        });
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).expect("read");
        handle.join().expect("join");
        let header = String::from_utf8_lossy(&buf);
        // "hello \u{2014} world" is 13 chars but 15 bytes (em dash is
        // 3 bytes in UTF-8). The header must report 15, not 13.
        assert!(
            header.contains("Content-Length: 15"),
            "Content-Length should be byte count, got: {header}"
        );
    }
}
