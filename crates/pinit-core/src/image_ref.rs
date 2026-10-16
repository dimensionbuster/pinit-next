//! What the user asked to open, independent of where the request came from.

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageRef {
    /// Remote image. `referer` helps with hotlink-protected hosts.
    Url { url: String, referer: Option<String> },
    File(PathBuf),
    /// Bytes already fetched by the browser (login/hotlink/blob safe).
    Bytes { data: Arc<[u8]>, mime: String },
}

/// Video files PinIt plays itself (what WebView2 decodes without extra codecs).
pub const VIDEO_EXTS: [&str; 5] = ["mp4", "webm", "m4v", "mov", "ogv"];

pub fn is_video_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

impl ImageRef {
    /// A local video may be opened in several windows at once (e.g. to compare positions);
    /// every other image is opened once and a repeat request just finds the existing window.
    pub fn allows_duplicates(&self) -> bool {
        matches!(self, ImageRef::File(p) if is_video_path(p))
    }

    /// Stable identity used to avoid opening the same image twice.
    pub fn key(&self) -> String {
        match self {
            ImageRef::Url { url, .. } => format!("url:{}", normalize_url(url)),
            ImageRef::File(p) => {
                format!("file:{}", p.to_string_lossy().replace('\\', "/").to_lowercase())
            }
            ImageRef::Bytes { data, .. } => format!("bytes:{:016x}:{}", fnv1a(data), data.len()),
        }
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn is_tracking_param(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("utm_")
        || matches!(n.as_str(), "fbclid" | "gclid" | "igshid" | "mc_cid" | "mc_eid")
}

/// Lowercase scheme+host, drop fragment and tracking params, keep the rest in order.
pub fn normalize_url(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw.split('#').next().unwrap_or(raw);
    let (base, query) = match raw.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (raw, None),
    };
    let base = match base.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = match rest.split_once('/') {
                Some((h, p)) => (h, format!("/{p}")),
                None => (rest, String::new()),
            };
            format!("{}://{}{}", scheme.to_ascii_lowercase(), host.to_ascii_lowercase(), path)
        }
        None => base.to_string(),
    };
    let kept: Vec<&str> = query
        .map(|q| {
            q.split('&')
                .filter(|kv| {
                    !kv.is_empty() && !is_tracking_param(kv.split('=').next().unwrap_or(""))
                })
                .collect()
        })
        .unwrap_or_default();
    if kept.is_empty() { base } else { format!("{base}?{}", kept.join("&")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(u: &str) -> ImageRef {
        ImageRef::Url { url: u.into(), referer: None }
    }

    #[test]
    fn tracking_params_and_fragment_do_not_change_identity() {
        assert_eq!(
            url("HTTPS://Example.com/a/b.jpg?utm_source=x&w=2&fbclid=9#top").key(),
            url("https://example.com/a/b.jpg?w=2").key()
        );
    }

    #[test]
    fn path_case_is_significant() {
        assert_ne!(url("https://e.com/A.jpg").key(), url("https://e.com/a.jpg").key());
    }

    #[test]
    fn referer_is_not_part_of_identity() {
        let a = ImageRef::Url {
            url: "https://e.com/a.jpg".into(),
            referer: Some("https://e.com/".into()),
        };
        assert_eq!(a.key(), url("https://e.com/a.jpg").key());
    }

    #[test]
    fn files_compare_case_and_slash_insensitively() {
        assert_eq!(
            ImageRef::File(PathBuf::from(r"C:\Pics\A.png")).key(),
            ImageRef::File(PathBuf::from("c:/pics/a.PNG")).key()
        );
    }

    #[test]
    fn bytes_identity_follows_content() {
        let a = ImageRef::Bytes { data: Arc::from(&b"abc"[..]), mime: "image/png".into() };
        let b = ImageRef::Bytes { data: Arc::from(&b"abc"[..]), mime: "image/png".into() };
        let c = ImageRef::Bytes { data: Arc::from(&b"abd"[..]), mime: "image/png".into() };
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), c.key());
    }
}
