//! Blocking Thingiverse REST client. Call only from worker threads.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::models::{Image, SearchPage, Thing, ThingDetails, ThingFile, User, lenient_vec};

pub const BASE_URL: &str = "https://api.thingiverse.com";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("The API token was rejected. Sign in again from the File menu.")]
    Unauthorized,
    #[error("Access to this item is forbidden.")]
    Forbidden,
    #[error("Not found.")]
    NotFound,
    #[error("Thingiverse is rate limiting requests. Wait a minute and try again.")]
    RateLimited,
    #[error("Server error {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Network error: {0}")]
    Network(String),
    #[error("Unexpected response from Thingiverse: {0}")]
    Decode(String),
    #[error("File error: {0}")]
    Io(#[from] io::Error),
    #[error("This file has no download link.")]
    NoDownloadUrl,
}

impl From<ureq::Error> for ApiError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Json(e) => ApiError::Decode(e.to_string()),
            ureq::Error::Io(e) => ApiError::Network(e.to_string()),
            other => ApiError::Network(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// Cloneable handle; the underlying agent shares its connection pool.
#[derive(Clone)]
pub struct Client {
    agent: Agent,
    download_agent: Agent,
    token: String,
}

pub(crate) fn agent(recv_timeout: Option<Duration>) -> Agent {
    agent_with_redirects(recv_timeout, 10)
}

fn agent_with_redirects(recv_timeout: Option<Duration>, max_redirects: u32) -> Agent {
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    Agent::config_builder()
        .tls_config(tls)
        .max_redirects(max_redirects)
        .http_status_as_error(false)
        .https_only(true)
        .user_agent(concat!("TV-Blind/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(recv_timeout)
        .build()
        .into()
}

/// Percent-encodes a single URL path segment.
fn encode_segment(s: &str) -> String {
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

/// Extracts a readable message from an error body, which may be JSON
/// (`{"error": "..."}`) or HTML.
fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["error", "message", "detail"] {
            if let Some(s) = v.get(key).and_then(|m| m.as_str()) {
                return s.to_string();
            }
        }
    }
    let text = body.trim();
    if text.starts_with('<') || text.is_empty() { "no details".to_string() } else { text.chars().take(200).collect() }
}

fn check_status(status: u16, body: impl FnOnce() -> String) -> Result<()> {
    match status {
        200..=299 => Ok(()),
        401 => Err(ApiError::Unauthorized),
        403 => Err(ApiError::Forbidden),
        404 => Err(ApiError::NotFound),
        429 => Err(ApiError::RateLimited),
        s => Err(ApiError::Http { status: s, message: error_message(&body()) }),
    }
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            agent: agent(Some(Duration::from_mins(1))),
            // Large files may take a while; only the connect/header phases time
            // out. Redirects are followed by hand: ureq 3.4 fails with
            // "chunk expected crlf" when it follows the API's chunked 302
            // itself, while the same hops done one by one work.
            download_agent: agent_with_redirects(None, 0),
            token: token.into(),
        }
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token.trim())
    }

    fn get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let mut req = self
            .agent
            .get(format!("{BASE_URL}{path}"))
            .header("Authorization", self.auth())
            .header("Accept", "application/json");
        for (k, v) in query {
            req = req.query(*k, v);
        }
        let mut resp = req.call()?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().with_config().limit(20 * 1024 * 1024).read_to_string()?;
        check_status(status, || text.clone())?;
        serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
    }

    /// `GET /search/{term}/?type=things`. `page` is 1-based.
    pub fn search_things(&self, term: &str, page: u32, per_page: u32) -> Result<SearchPage> {
        let path = format!("/search/{}/", encode_segment(term.trim()));
        let v: serde_json::Value = self.get_json(
            &path,
            &[
                ("type", "things".into()),
                ("page", page.to_string()),
                ("per_page", per_page.to_string()),
                ("sort", "relevant".into()),
            ],
        )?;
        // Documented shape is {total, hits}; older endpoints return a bare array.
        let (total, hits) = match v {
            serde_json::Value::Array(_) => (None, v),
            serde_json::Value::Object(mut o) => {
                let total = o.get("total").and_then(|t| t.as_u64().or_else(|| t.as_str()?.parse().ok()));
                (total, o.remove("hits").unwrap_or_default())
            }
            _ => (None, serde_json::Value::Null),
        };
        let hits: Vec<Thing> = lenient_vec(hits).map_err(|e| ApiError::Decode(e.to_string()))?;
        Ok(SearchPage { term: term.to_string(), page, per_page, total, hits })
    }

    /// The signed-in user.
    pub fn me(&self) -> Result<User> {
        self.get_json("/users/me", &[])
    }

    pub fn thing(&self, id: u64) -> Result<Thing> {
        self.get_json(&format!("/things/{id}"), &[])
    }

    pub fn files(&self, thing_id: u64) -> Result<Vec<ThingFile>> {
        let v: serde_json::Value = self.get_json(&format!("/things/{thing_id}/files"), &[])?;
        lenient_vec(v).map_err(|e| ApiError::Decode(e.to_string()))
    }

    pub fn images(&self, thing_id: u64) -> Result<Vec<Image>> {
        let v: serde_json::Value = self.get_json(&format!("/things/{thing_id}/images"), &[])?;
        lenient_vec(v).map_err(|e| ApiError::Decode(e.to_string()))
    }

    /// Thing, files and images. Missing images are not fatal.
    pub fn details(&self, id: u64) -> Result<ThingDetails> {
        let thing = self.thing(id)?;
        let files = self.files(id)?;
        let images = self.images(id).unwrap_or_default();
        Ok(ThingDetails { thing, files, images })
    }

    /// Downloads `file` into `dir`, returning the final path. Writes to a
    /// `.part` file first so an interrupted download never looks complete.
    /// `progress` receives (bytes so far, total if known).
    pub fn download_file(
        &self,
        file: &ThingFile,
        dir: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let dest = unique_path(dir, &safe_file_name(&file.display_name()));

        // Prefer the tracked, authenticated endpoint; it redirects to the CDN
        // (see `fetch_to`, which keeps the token off the CDN). Fall back to the
        // direct links the file record carries.
        let mut candidates: Vec<(String, bool)> = Vec::new();
        if let Some(id) = file.id {
            candidates.push((format!("{BASE_URL}/files/{id}/download"), true));
        }
        for u in [&file.direct_url, &file.public_url, &file.download_url].into_iter().flatten() {
            candidates.push((u.clone(), u.starts_with(BASE_URL)));
        }
        if candidates.is_empty() {
            return Err(ApiError::NoDownloadUrl);
        }

        let part = dest.with_extension(match dest.extension() {
            Some(e) => format!("{}.part", e.to_string_lossy()),
            None => "part".into(),
        });
        let mut last_err = ApiError::NoDownloadUrl;
        for (url, with_auth) in candidates {
            match self.fetch_to(&url, with_auth, &part, file.size, &mut progress) {
                Ok(()) => {
                    fs::rename(&part, &dest)?;
                    return Ok(dest);
                }
                Err(e) => {
                    let _ = fs::remove_file(&part);
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    /// Follows up to five redirects by hand and streams the final response
    /// into `part`. The token only goes to the API host, never to the CDN.
    fn fetch_to(
        &self,
        url: &str,
        with_auth: bool,
        part: &Path,
        size_hint: Option<u64>,
        progress: &mut impl FnMut(u64, Option<u64>),
    ) -> Result<()> {
        let mut url = url.to_string();
        let mut resp = None;
        for _ in 0..=5 {
            let mut req = self.download_agent.get(&url);
            if with_auth && is_api_url(&url) {
                req = req.header("Authorization", self.auth());
            }
            let r = req.call()?;
            if !r.status().is_redirection() {
                resp = Some(r);
                break;
            }
            let location = r
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ApiError::Decode("redirect without a location".into()))?;
            url = resolve_redirect(&url, location)?;
        }
        let mut resp = resp.ok_or_else(|| ApiError::Network("too many redirects".into()))?;

        check_status(resp.status().as_u16(), String::new)?;
        if resp.body().mime_type() == Some("text/html") {
            // A web page (login wall, etc.), not the file.
            return Err(ApiError::Decode("got a web page instead of the file".into()));
        }
        let total = resp.body().content_length().or(size_hint);
        let mut reader = resp.body_mut().as_reader();
        let mut out = File::create(part)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| ApiError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            progress(done, total);
        }
        out.sync_all()?;
        Ok(())
    }
}

fn is_api_url(url: &str) -> bool {
    url.starts_with(BASE_URL) && matches!(url.as_bytes().get(BASE_URL.len()), None | Some(b'/' | b'?'))
}

/// Absolute HTTPS URL for a `Location` header, which may be relative.
fn resolve_redirect(current: &str, location: &str) -> Result<String> {
    let next = if location.starts_with("https://") {
        location.to_string()
    } else if location.starts_with('/') && !location.starts_with("//") {
        // Scheme and host of the current URL, then the new path.
        let host_end = current.get(8..).and_then(|rest| rest.find('/')).map_or(current.len(), |i| i + 8);
        format!("{}{location}", &current[..host_end])
    } else {
        return Err(ApiError::Decode("redirect to an unsupported address".into()));
    };
    Ok(next)
}

pub fn safe_file_name(name: &str) -> String {
    let opts = sanitize_filename::Options { truncate: true, windows: true, replacement: "_" };
    let s = sanitize_filename::sanitize_with_options(name.trim(), opts).into_owned();
    if s.trim_matches(['.', ' ', '_']).is_empty() { "download".to_string() } else { s }
}

/// `dir/name`, or `dir/name (2).ext` etc. if that already exists.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    // Bounded so a pathological folder can't loop forever; fall back to the
    // last candidate, which the caller simply overwrites.
    (2..10_000u32)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .unwrap_or_else(|| dir.join(format!("{stem} (10000){ext}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_path_segment() {
        assert_eq!(encode_segment("benchy boat/2"), "benchy%20boat%2F2");
        assert_eq!(encode_segment("süß"), "s%C3%BC%C3%9F");
    }

    #[test]
    fn redirects_resolve_and_auth_stays_on_api_host() {
        let cur = "https://api.thingiverse.com/files/1/download";
        assert_eq!(
            resolve_redirect(cur, "https://cdn.thingiverse.com/a.stl").unwrap(),
            "https://cdn.thingiverse.com/a.stl"
        );
        assert_eq!(resolve_redirect(cur, "/files/2/download").unwrap(), "https://api.thingiverse.com/files/2/download");
        assert!(resolve_redirect(cur, "http://cdn.thingiverse.com/a.stl").is_err());
        assert!(resolve_redirect(cur, "//evil.example/a").is_err());
        assert!(is_api_url(cur));
        assert!(!is_api_url("https://cdn.thingiverse.com/a.stl"));
        assert!(!is_api_url("https://api.thingiverse.com.evil.example/x"));
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(safe_file_name("a:b?.stl"), "a_b_.stl");
        assert_eq!(safe_file_name("..."), "download");
    }

    /// Network check that needs no token: a bogus one must map to Unauthorized.
    #[test]
    #[ignore = "needs network"]
    fn live_bad_token_is_unauthorized() {
        let started = std::time::Instant::now();
        let r = Client::new("not-a-real-token").search_things("benchy", 1, 1);
        assert!(matches!(r, Err(ApiError::Unauthorized)), "{r:?}");
        println!("took {:?}", started.elapsed());
    }

    /// Hits the real API with the token saved by the app (or `THINGIVERSE_TOKEN`).
    /// Run with `cargo test -- --ignored live_api --nocapture`.
    #[test]
    #[ignore = "needs network and a Thingiverse token"]
    fn live_api() {
        let token = std::env::var("THINGIVERSE_TOKEN")
            .ok()
            .or_else(crate::config::load_token)
            .expect("no token: run the app once or set THINGIVERSE_TOKEN");
        let c = Client::new(token);

        // 3DBenchy.
        let d = c.details(763_622).expect("details");
        assert_eq!(d.thing.id, Some(763_622));
        println!("thing: {} by {}", d.thing.title(), d.thing.creator_name());
        assert!(!d.files.is_empty(), "benchy has files");
        for f in &d.files {
            println!("  file: {} ({})", f.display_name(), f.size_text());
        }
        println!("  images: {}", d.images.len());

        let s = c.search_things("benchy", 1, 5).expect("search");
        println!("search total {:?}, got {}", s.total, s.hits.len());
        assert!(!s.hits.is_empty());

        let dir = std::env::temp_dir().join("tvb-live-test");
        let _ = fs::remove_dir_all(&dir);
        let smallest = d.files.iter().min_by_key(|f| f.size.unwrap_or(u64::MAX)).unwrap();
        let path = c.download_file(smallest, &dir, |_, _| {}).expect("download");
        let len = fs::metadata(&path).unwrap().len();
        println!("downloaded {} ({len} bytes)", path.display());
        assert!(len > 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
