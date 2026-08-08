//! OAuth2 PKCE login for OpenRouter.
//!
//! The flow mirrors what `pi-mono` does: generate a PKCE pair, bind a
//! loopback callback server, hand the user an authorize URL, and exchange
//! the captured authorization code for a permanent OpenRouter API key at
//! `/api/v1/auth/keys`. OpenRouter does not issue a client secret for the
//! PKCE flow, so the only secrets involved are the verifier (kept in
//! memory) and the resulting key (saved via `save_key`).

use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::oauth::{bind_loopback, generate_pkce, CallbackResult};

const AUTHORIZE_URL: &str = "https://openrouter.ai/auth";
const TOKEN_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
const CALLBACK_PATH: &str = "/oauth/callback/plasma";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Run the full OpenRouter OAuth flow. Returns the permanent API key that
/// can be used as a Bearer token against the OpenRouter REST API.
pub fn login<F>(open_browser: F) -> Result<String>
where
    F: FnOnce(&str),
{
    let pkce = generate_pkce();
    let (port, rx) = bind_loopback(CALLBACK_PATH, LOGIN_TIMEOUT)?;
    let callback_url = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");

    let authorize_url = format!(
        "{AUTHORIZE_URL}?callback_url={callback}&code_challenge={challenge}&code_challenge_method=S256",
        callback = url_encode(&callback_url),
        challenge = url_encode(&pkce.challenge),
    );

    open_browser(&authorize_url);

    let outcome = rx
        .recv_timeout(LOGIN_TIMEOUT)
        .map_err(|_| anyhow!("Timed out waiting for the OpenRouter OAuth callback"))?;
    let code = match outcome {
        CallbackResult::Code(code) => code,
    };

    exchange_code(&code, &pkce.verifier)
}

fn exchange_code(code: &str, verifier: &str) -> Result<String> {
    let response = ureq::post(TOKEN_URL)
        .set("accept", "application/json")
        .set("content-type", "application/json")
        .send_json(ureq::json!({
            "code": code,
            "code_verifier": verifier,
            "code_challenge_method": "S256",
        }))
        .map_err(|error| anyhow!("OpenRouter key exchange request failed: {error}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .into_json()
        .map_err(|error| anyhow!("OpenRouter key exchange returned invalid JSON: {error}"))?;
    if !(200..300).contains(&status) {
        let detail = body
            .get("error")
            .or_else(|| body.get("message"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow!(
            "OpenRouter OAuth key exchange failed (HTTP {status}): {detail}"
        ));
    }
    let key = body
        .get("key")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("OpenRouter OAuth response did not include a key"))?;
    Ok(key.to_string())
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_passes_through_unreserved() {
        assert_eq!(
            url_encode("http://127.0.0.1:12345/oauth/callback/plasma"),
            "http%3A%2F%2F127.0.0.1%3A12345%2Foauth%2Fcallback%2Fplasma"
        );
        assert_eq!(url_encode("abc-DEF_123.~"), "abc-DEF_123.~");
    }
}
