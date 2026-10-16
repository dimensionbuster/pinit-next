//! Image resolvers: turn an `ImageRef` into a local file with a known size.
//! Remote and in-memory images are cached on disk so the viewer only ever
//! loads local files and restoring a layout needs no network.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pinit_core::image_ref::ImageRef;
use pinit_core::image_size::image_size;
use pinit_core::ports::{ImageResolver, ResolveError, ResolvedImage};

const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const HEADER_BYTES: usize = 256 * 1024;
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

fn fail(msg: impl Into<String>) -> ResolveError {
    ResolveError::Failed(msg.into())
}

fn resolved(image: &ImageRef, path: PathBuf, header: &[u8]) -> Result<ResolvedImage, ResolveError> {
    let (width, height) = image_size(header).ok_or_else(|| fail("not a supported image"))?;
    Ok(ResolvedImage { image: image.clone(), local_path: path, width, height })
}

fn read_header(path: &Path) -> Result<Vec<u8>, ResolveError> {
    let mut f = fs::File::open(path).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    let mut buf = vec![0u8; HEADER_BYTES];
    let n = f.read(&mut buf).map_err(|e| fail(e.to_string()))?;
    buf.truncate(n);
    Ok(buf)
}

fn extension_for(header: &[u8]) -> &'static str {
    match header {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [b'G', b'I', b'F', ..] => "gif",
        [0xFF, 0xD8, ..] => "jpg",
        [b'B', b'M', ..] => "bmp",
        [b'R', b'I', b'F', b'F', ..] => "webp",
        _ => "img",
    }
}

fn cache_name(key: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Finds an already cached file for `key` regardless of its extension.
fn cached(dir: &Path, key: &str) -> Option<PathBuf> {
    let stem = cache_name(key);
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_stem().is_some_and(|s| s.to_string_lossy() == stem)
    })
}

fn store(dir: &Path, key: &str, data: &[u8]) -> Result<PathBuf, ResolveError> {
    fs::create_dir_all(dir).map_err(|e| fail(e.to_string()))?;
    let path = dir.join(format!("{}.{}", cache_name(key), extension_for(data)));
    fs::write(&path, data).map_err(|e| fail(e.to_string()))?;
    Ok(path)
}

pub struct FileResolver;

impl ImageResolver for FileResolver {
    fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
        let ImageRef::File(path) = image else { return Err(ResolveError::Unsupported) };
        if crate::cli::is_video_path(path) {
            // The real size is only known once the viewer has loaded the video: it reports it
            // back (`viewer_video_size`) and the window is refitted. Until then, 16:9.
            fs::metadata(path).map_err(|e| fail(format!("{}: {e}", path.display())))?;
            return Ok(ResolvedImage { image: image.clone(), local_path: path.clone(), width: 1280, height: 720 });
        }
        let header = read_header(path)?;
        resolved(image, path.clone(), &header)
    }
}

pub struct BytesResolver {
    pub cache_dir: PathBuf,
}

impl ImageResolver for BytesResolver {
    fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
        let ImageRef::Bytes { data, .. } = image else { return Err(ResolveError::Unsupported) };
        let key = image.key();
        let path = match cached(&self.cache_dir, &key) {
            Some(p) => p,
            None => store(&self.cache_dir, &key, data)?,
        };
        resolved(image, path, data)
    }
}

pub struct UrlResolver {
    pub cache_dir: PathBuf,
}

impl ImageResolver for UrlResolver {
    fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
        let ImageRef::Url { url, referer } = image else { return Err(ResolveError::Unsupported) };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ResolveError::Unsupported);
        }
        let key = image.key();
        if let Some(path) = cached(&self.cache_dir, &key) {
            let header = read_header(&path)?;
            return resolved(image, path, &header);
        }

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let mut req = agent.get(url).header("User-Agent", USER_AGENT).header(
            "Accept",
            "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
        );
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        let mut res = req.call().map_err(|e| fail(format!("{url}: {e}")))?;
        let data = res
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE_BYTES)
            .read_to_vec()
            .map_err(|e| fail(format!("{url}: {e}")))?;
        let (_w, _h) = image_size(&data).ok_or_else(|| fail("response is not a supported image"))?;
        let path = store(&self.cache_dir, &key, &data)?;
        resolved(image, path, &data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::Arc;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 2, 0, 0, 0]);
        v
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pinit-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn file_resolver_reads_size_and_ignores_other_refs() {
        let d = temp_dir("file");
        let p = d.join("a.png");
        fs::write(&p, png(120, 80)).unwrap();
        let r = FileResolver.resolve(&ImageRef::File(p.clone())).unwrap();
        assert_eq!((r.width, r.height, r.local_path), (120, 80, p));
        let url = ImageRef::Url { url: "http://x/a.png".into(), referer: None };
        assert_eq!(FileResolver.resolve(&url), Err(ResolveError::Unsupported));
    }

    #[test]
    fn file_resolver_rejects_non_images() {
        let d = temp_dir("notimg");
        let p = d.join("a.txt");
        fs::write(&p, b"hello").unwrap();
        assert!(matches!(FileResolver.resolve(&ImageRef::File(p)), Err(ResolveError::Failed(_))));
    }

    #[test]
    fn bytes_resolver_caches_once() {
        let d = temp_dir("bytes");
        let r = BytesResolver { cache_dir: d.clone() };
        let img = ImageRef::Bytes { data: Arc::from(png(30, 60)), mime: "image/png".into() };
        let a = r.resolve(&img).unwrap();
        let b = r.resolve(&img).unwrap();
        assert_eq!(a.local_path, b.local_path);
        assert_eq!((a.width, a.height), (30, 60));
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
    }

    #[test]
    fn url_resolver_downloads_sends_referer_then_serves_from_cache() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = png(200, 100);
        let server = std::thread::spawn(move || {
            // serve exactly one request; a second would hang the test
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
            let status = if req.contains("referer: https://site.example/") { "200 OK" } else { "403 Forbidden" };
            let payload: &[u8] = if status == "200 OK" { &body } else { b"" };
            write!(s, "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len()).unwrap();
            s.write_all(payload).unwrap();
        });
        let d = temp_dir("url");
        let r = UrlResolver { cache_dir: d };
        let img = ImageRef::Url {
            url: format!("http://127.0.0.1:{port}/p.png"),
            referer: Some("https://site.example/".into()),
        };
        let first = r.resolve(&img).unwrap();
        assert_eq!((first.width, first.height), (200, 100));
        server.join().unwrap();
        // server is gone: second call must come from cache
        let second = r.resolve(&img).unwrap();
        assert_eq!(first.local_path, second.local_path);
    }

    #[test]
    fn url_resolver_skips_non_http() {
        let r = UrlResolver { cache_dir: temp_dir("skip") };
        let img = ImageRef::Url { url: "data:image/png;base64,AAAA".into(), referer: None };
        assert_eq!(r.resolve(&img), Err(ResolveError::Unsupported));
    }
}
