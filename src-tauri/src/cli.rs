//! Command-line / second-instance argument parsing into open requests.
//!
//! Accepted:
//!   `<image-file>...`                      local images (file association, shell menu)
//!   `--open-url <url> [--referer <url>]`   remote image (used by the browser bridge)

use std::path::PathBuf;

use pinit_core::image_ref::ImageRef;

const IMAGE_EXTS: [&str; 7] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "jfif"];

pub fn is_image_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Video files PinIt plays itself (what WebView2 decodes without extra codecs).
pub const VIDEO_EXTS: [&str; 5] = ["mp4", "webm", "m4v", "mov", "ogv"];

pub fn is_video_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// True when the launch came from the browser bridge (an extension hello or an open request).
pub fn from_browser(args: &[String]) -> bool {
    has_flag(args, "--bridge-hello") || has_flag(args, "--open-url")
}

/// `args` excludes the executable name.
pub fn parse_open_args(args: &[String]) -> Vec<ImageRef> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--open-url" => {
                if let Some(url) = args.get(i + 1) {
                    let referer = match (args.get(i + 2).map(String::as_str), args.get(i + 3)) {
                        (Some("--referer"), Some(r)) => {
                            i += 2;
                            Some(r.clone())
                        }
                        _ => None,
                    };
                    out.push(ImageRef::Url { url: url.clone(), referer });
                    i += 1;
                }
            }
            a if a.starts_with('-') => {}
            a => {
                let p = PathBuf::from(a);
                if (is_image_path(&p) || is_video_path(&p)) && p.is_file() {
                    out.push(ImageRef::File(p));
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn url_with_and_without_referer() {
        assert_eq!(
            parse_open_args(&a(&["--open-url", "https://e.com/a.jpg", "--referer", "https://e.com/"])),
            vec![ImageRef::Url { url: "https://e.com/a.jpg".into(), referer: Some("https://e.com/".into()) }]
        );
        assert_eq!(
            parse_open_args(&a(&["--open-url", "https://e.com/a.jpg"])),
            vec![ImageRef::Url { url: "https://e.com/a.jpg".into(), referer: None }]
        );
    }

    #[test]
    fn files_must_exist_and_be_images() {
        let dir = std::env::temp_dir().join(format!("pinit-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("a.PNG");
        let txt = dir.join("a.txt");
        std::fs::write(&img, b"x").unwrap();
        std::fs::write(&txt, b"x").unwrap();
        let args = a(&[
            img.to_str().unwrap(),
            txt.to_str().unwrap(),
            "missing.png",
            "--unknown-flag",
        ]);
        assert_eq!(parse_open_args(&args), vec![ImageRef::File(img)]);
    }

    #[test]
    fn video_files_are_accepted() {
        let dir = std::env::temp_dir().join(format!("pinit-cli-v-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = dir.join("clip.MP4");
        std::fs::write(&v, b"x").unwrap();
        assert_eq!(parse_open_args(&a(&[v.to_str().unwrap()])), vec![ImageRef::File(v)]);
    }

    #[test]
    fn browser_launches_are_recognised() {
        assert!(from_browser(&a(&["--bridge-hello"])));
        assert!(from_browser(&a(&["--open-url", "https://e.com/a.jpg"])));
        assert!(!from_browser(&a(&["C:/a.png"])));
        assert!(parse_open_args(&a(&["--bridge-hello"])).is_empty());
    }

    #[test]
    fn dangling_flag_is_ignored() {
        assert!(parse_open_args(&a(&["--open-url"])).is_empty());
    }
}
