use std::{
    collections::HashMap,
    io::Cursor,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{imageops::FilterType, ImageFormat};
use regex::Regex;
use reqwest::redirect::Policy;
use tokio::sync::RwLock;

/// Square pixel size we normalize favicons to before encoding them for the UI.
const ICON_SIZE: u32 = 32;

const SUCCESS_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const FAILURE_TTL: Duration = Duration::from_secs(5 * 60);

struct CacheEntry {
    /// `Some` holds a `data:image/png;base64,...` URL ready for an `<img>` tag.
    /// `None` records that we tried and found nothing (so we back off).
    data_url: Option<String>,
    fetched_at: Instant,
}

impl CacheEntry {
    fn is_fresh(&self) -> bool {
        let ttl = if self.data_url.is_some() {
            SUCCESS_TTL
        } else {
            FAILURE_TTL
        };
        self.fetched_at.elapsed() < ttl
    }
}

/// In-memory favicon cache keyed by service origin (e.g. `http://192.168.1.10:8080`).
/// Shared via `AppState`; survives for the app session only.
#[derive(Default)]
pub struct FaviconStore {
    inner: RwLock<HashMap<String, CacheEntry>>,
}

impl FaviconStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a PNG data URL for `origin`'s favicon, fetching on a cache miss.
    /// `None` means the service has no usable favicon.
    pub async fn get(&self, origin: &str, http_timeout_ms: u64) -> Option<String> {
        if let Some(entry) = self.inner.read().await.get(origin) {
            if entry.is_fresh() {
                return entry.data_url.clone();
            }
        }

        let data_url = fetch_favicon(origin, http_timeout_ms).await;
        self.inner.write().await.insert(
            origin.to_string(),
            CacheEntry {
                data_url: data_url.clone(),
                fetched_at: Instant::now(),
            },
        );
        data_url
    }
}

async fn fetch_favicon(origin: &str, http_timeout_ms: u64) -> Option<String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(Policy::limited(3))
        .timeout(Duration::from_millis(http_timeout_ms))
        .build()
        .ok()?;

    if let Some(url) = discover_icon_url(&client, origin).await {
        if let Some(data_url) = download_and_decode(&client, &url).await {
            return Some(data_url);
        }
    }

    download_and_decode(&client, &format!("{origin}/favicon.ico")).await
}

/// Parses the document at `origin` for a `<link rel="...icon...">` and resolves it
/// to an absolute URL. Returns `None` when the page can't be read or has no hint.
async fn discover_icon_url(client: &reqwest::Client, origin: &str) -> Option<String> {
    let body = client
        .get(&format!("{origin}/"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let re = Regex::new(
        r#"(?is)<link[^>]+rel=["'][^"']*icon[^"']*["'][^>]*href=["'](?P<href>[^"']+)["']"#,
    )
    .ok()?;
    let alt = Regex::new(
        r#"(?is)<link[^>]+href=["'](?P<href>[^"']+)["'][^>]*rel=["'][^"']*icon[^"']*["']"#,
    )
    .ok()?;

    let href = re
        .captures(&body)
        .or_else(|| alt.captures(&body))
        .and_then(|caps| caps.name("href"))
        .map(|m| m.as_str().trim().to_string())?;

    Some(resolve_url(origin, &href))
}

fn resolve_url(origin: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        let scheme = origin.split(':').next().unwrap_or("http");
        format!("{scheme}://{rest}")
    } else if href.starts_with('/') {
        format!("{origin}{href}")
    } else {
        format!("{origin}/{href}")
    }
}

async fn download_and_decode(client: &reqwest::Client, url: &str) -> Option<String> {
    let bytes = client.get(url).send().await.ok()?.bytes().await.ok()?;
    encode_data_url(&bytes)
}

/// Decodes any common image format, normalizes to a square PNG, and returns it
/// as a base64 `data:` URL the webview can render directly.
fn encode_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let image = image::load_from_memory(bytes).ok()?;
    let resized = image.resize_exact(ICON_SIZE, ICON_SIZE, FilterType::Lanczos3);

    let mut png = Cursor::new(Vec::new());
    resized.write_to(&mut png, ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    ))
}
