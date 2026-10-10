//! "Sign in with Thingiverse" through the browser.
//!
//! Uses the client-side (implicit) flow, `response_type=token`, so no client
//! secret ships in the exe. Thingiverse redirects the browser to a one-shot
//! listener on 127.0.0.1 with the token in the URL fragment. Browsers never
//! send fragments to servers, so the callback page posts it back with a
//! little script. The token is then checked against `tokeninfo` to confirm
//! it was issued to this app.

use std::collections::hash_map::RandomState;
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::api::{ApiError, Client};

/// Must match the callback URL registered for the app on Thingiverse.
pub const CALLBACK_PORT: u16 = 47823;
const AUTHORIZE_URL: &str = "https://www.thingiverse.com/login/oauth/authorize";
const TOKENINFO_URL: &str = "https://www.thingiverse.com/login/oauth/tokeninfo";

/// The registered app's client ID, baked in at build time (see
/// `.cargo/config.toml`). Client IDs are public; there is no secret.
pub fn client_id() -> Option<&'static str> {
    option_env!("TVB_CLIENT_ID").map(str::trim).filter(|s| !s.is_empty())
}

pub fn redirect_uri() -> String {
    format!("http://127.0.0.1:{CALLBACK_PORT}/callback")
}

#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error("Browser sign-in is not set up in this build.")]
    NotConfigured,
    #[error("Port {CALLBACK_PORT} is in use by another program, so sign-in cannot receive the reply ({0}).")]
    PortInUse(io::Error),
    #[error("Could not open the web browser: {0}")]
    Browser(String),
    #[error("Sign-in timed out. Try again.")]
    TimedOut,
    #[error("Sign-in canceled.")]
    Canceled,
    #[error("Thingiverse did not grant access: {0}")]
    Denied(String),
    #[error("The token was issued to a different app.")]
    WrongApp,
    #[error("Could not check the token: {0}")]
    Api(#[from] ApiError),
}

pub struct SignedIn {
    pub token: String,
    pub user_name: Option<String>,
}

/// Runs the whole flow. Blocks; call from a worker thread. Returns early with
/// `Canceled` once `cancel` is set.
pub fn sign_in(cancel: &AtomicBool, timeout: Duration) -> Result<SignedIn, SignInError> {
    let client_id = client_id().ok_or(SignInError::NotConfigured)?;
    let listener = bind_with_retry()?;
    listener.set_nonblocking(true).map_err(SignInError::PortInUse)?;

    let state = random_state();
    let url = format!(
        "{AUTHORIZE_URL}?client_id={}&redirect_uri={}&response_type=token&state={state}",
        encode(client_id),
        encode(&redirect_uri()),
    );
    opener::open_browser(&url).map_err(|e| SignInError::Browser(e.to_string()))?;

    let token = wait_for_token(&listener, &state, cancel, timeout)?;
    drop(listener);

    check_audience(&token, client_id)?;
    let user_name = Client::new(token.clone()).me().ok().and_then(|u| u.display_name());
    Ok(SignedIn { token, user_name })
}

/// A previous, just-canceled attempt may still hold the port briefly.
fn bind_with_retry() -> Result<TcpListener, SignInError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match TcpListener::bind(("127.0.0.1", CALLBACK_PORT)) {
            Ok(l) => return Ok(l),
            Err(e) if Instant::now() >= deadline => return Err(SignInError::PortInUse(e)),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn wait_for_token(
    listener: &TcpListener,
    state: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<String, SignInError> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(SignInError::Canceled);
        }
        if Instant::now() >= deadline {
            return Err(SignInError::TimedOut);
        }
        match listener.accept() {
            Ok((stream, _)) => {
                // A malformed or stray request must not end the sign-in.
                if let Ok(Some(outcome)) = handle(stream, state) {
                    return outcome;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(100)),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct Request {
    method: String,
    path: String,
    query: String,
    body: String,
}

fn read_request(stream: &TcpStream) -> io::Result<Request> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.take(16 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (path, query) = (path.to_string(), query.to_string());

    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = v.trim().parse().unwrap_or(0).min(8 * 1024);
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, query, body: String::from_utf8_lossy(&body).into_owned() })
}

/// Serves one request. `Some(result)` ends the flow.
fn handle(mut stream: TcpStream, state: &str) -> io::Result<Option<Result<String, SignInError>>> {
    let req = read_request(&stream)?;
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/callback") => {
            let q = parse_form(&req.query);
            if let Some(err) = get(&q, "error") {
                let why = get(&q, "error_description").unwrap_or(err).to_string();
                respond(&mut stream, 200, &page("Sign-in failed", &format!("{why}. Return to TV-Blind.")))?;
                return Ok(Some(Err(SignInError::Denied(why))));
            }
            respond(&mut stream, 200, CALLBACK_PAGE)?;
            Ok(None)
        }
        ("POST", "/token") => {
            let f = parse_form(&req.body);
            let outcome = match (get(&f, "access_token"), get(&f, "state"), get(&f, "error")) {
                (_, _, Some(err)) => Err(SignInError::Denied(err.to_string())),
                // Thingiverse may drop `state`; if it is echoed it must match.
                (Some(_), Some(s), _) if s != state => Err(SignInError::Denied("state mismatch".into())),
                (Some(t), _, _) => Ok(t.to_string()),
                (None, _, _) => Err(SignInError::Denied("no token in the reply".into())),
            };
            let text = if outcome.is_ok() { "ok" } else { "error" };
            respond_plain(&mut stream, text)?;
            Ok(Some(outcome))
        }
        _ => {
            respond(&mut stream, 404, &page("Not found", "Nothing here."))?;
            Ok(None)
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, html: &str) -> io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         Connection: close\r\n\r\n{html}",
        html.len()
    )?;
    stream.flush()
}

fn respond_plain(stream: &mut TcpStream, text: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )?;
    stream.flush()
}

fn page(title: &str, message: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body><main><h1>{title}</h1><p>{message}</p></main></body></html>"
    )
}

/// Moves the fragment out of the address bar (and history) and posts it to
/// the app. The result is written into a heading so screen readers find it.
const CALLBACK_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>TV-Blind sign-in</title></head>
<body><main><h1 id="msg" role="status">Finishing sign-in...</h1></main>
<script>
var frag = location.hash.replace(/^#/, "");
history.replaceState(null, "", "/callback");
var msg = document.getElementById("msg");
function done(text) { msg.textContent = text; document.title = text; }
if (!frag) {
  done("No token was received. Return to TV-Blind and try again.");
} else {
  fetch("/token", { method: "POST", body: frag, headers: { "Content-Type": "application/x-www-form-urlencoded" } })
    .then(function (r) { return r.text(); })
    .then(function (t) {
      done(t === "ok" ? "Signed in. You can close this tab and return to TV-Blind."
                      : "Sign-in failed. Return to TV-Blind and try again.");
    })
    .catch(function () { done("Could not reach TV-Blind. Is it still running?"); });
}
</script></body></html>"#;

/// Confirms the token belongs to this app (prevents using a token some other
/// app obtained, the "confused deputy" case the docs warn about).
fn check_audience(token: &str, client_id: &str) -> Result<(), SignInError> {
    let mut resp = crate::api::agent(Some(Duration::from_secs(30)))
        .post(TOKENINFO_URL)
        .send_form([("access_token", token)])
        .map_err(ApiError::from)?;
    if crate::api::is_challenge(&resp) {
        return Err(ApiError::Blocked.into());
    }
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().map_err(ApiError::from)?;
    parse_tokeninfo(status, &text, client_id)
}

fn parse_tokeninfo(status: u16, text: &str, client_id: &str) -> Result<(), SignInError> {
    crate::api::check_status(status, || text.to_string())?;
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| ApiError::Decode(e.to_string()))?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(SignInError::Denied(err.to_string()));
    }
    let audience = match v.get("audience") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    if audience == client_id { Ok(()) } else { Err(SignInError::WrongApp) }
}

fn random_state() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut out = String::with_capacity(32);
    for i in 0..2u8 {
        // Each RandomState is freshly keyed from OS randomness.
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u8(i);
        let _ = write!(out, "{:016x}", h.finish());
    }
    out
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_form(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fragment_form() {
        let f = parse_form("access_token=abc%2F1&token_type=bearer&state=xy+z&empty=");
        assert_eq!(get(&f, "access_token"), Some("abc/1"));
        assert_eq!(get(&f, "state"), Some("xy z"));
        assert_eq!(get(&f, "empty"), None);
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%4"), "%4");
    }

    #[test]
    fn state_is_random_hex() {
        let (a, b) = (random_state(), random_state());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    /// Drives the local listener the way the browser would.
    #[test]
    fn listener_round_trip() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let cancel = AtomicBool::new(false);
        let browser = thread::spawn(move || {
            let send = |req: String| {
                let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.write_all(req.as_bytes()).unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).unwrap();
                out
            };
            let page = send("GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n".into());
            assert!(page.starts_with("HTTP/1.1 404"));
            let page = send("GET /callback HTTP/1.1\r\nHost: x\r\n\r\n".into());
            assert!(page.contains("location.hash"));
            let body = "access_token=tok123&state=s1";
            send(format!("POST /token HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
        });
        let token = wait_for_token(&listener, "s1", &cancel, Duration::from_secs(10)).unwrap();
        assert_eq!(token, "tok123");
        assert!(browser.join().unwrap().ends_with("ok"));
    }

    #[test]
    fn rejects_mismatched_state_and_honors_cancel() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let cancel = AtomicBool::new(false);
        thread::spawn(move || {
            let body = "access_token=t&state=evil";
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST /token HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            let _ = s.read_to_string(&mut String::new());
        });
        let r = wait_for_token(&listener, "good", &cancel, Duration::from_secs(10));
        assert!(matches!(r, Err(SignInError::Denied(_))));

        cancel.store(true, Ordering::SeqCst);
        let r = wait_for_token(&listener, "good", &cancel, Duration::from_secs(10));
        assert!(matches!(r, Err(SignInError::Canceled)));
    }

    #[test]
    fn tokeninfo_replies() {
        let html = "<!DOCTYPE html><title>Just a moment...</title>";
        assert!(matches!(parse_tokeninfo(403, html, "1"), Err(SignInError::Api(ApiError::Forbidden))));
        assert!(matches!(parse_tokeninfo(200, html, "1"), Err(SignInError::Api(ApiError::Decode(_)))));
        assert!(matches!(parse_tokeninfo(200, r#"{"error":"invalid token"}"#, "1"), Err(SignInError::Denied(_))));
        assert!(parse_tokeninfo(200, r#"{"audience":1}"#, "1").is_ok());
        assert!(parse_tokeninfo(200, r#"{"audience":"1"}"#, "1").is_ok());
        assert!(matches!(parse_tokeninfo(200, r#"{"audience":"2"}"#, "1"), Err(SignInError::WrongApp)));
        assert!(matches!(parse_tokeninfo(200, "{}", "1"), Err(SignInError::WrongApp)));
    }

    /// `cargo test -- --ignored live_tokeninfo`. Catches the tokeninfo host
    /// answering with something other than JSON (e.g. a bot challenge page).
    #[test]
    #[ignore = "needs network"]
    fn live_tokeninfo_rejects_bad_token() {
        let r = check_audience("not-a-real-token", "0");
        assert!(matches!(r, Err(SignInError::Denied(_))), "{r:?}");
    }
}
