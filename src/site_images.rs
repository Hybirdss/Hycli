//! Website identity images are discovered on the official page and cached locally.
//! The browser never loads a third-party image URL, and image fetches carry no sign-in.
use crate::{
    apperr::{AppError, AppResult},
    net::{self, Request},
    policy,
    runtime::Runtime,
    util,
};
use std::time::Duration;

pub fn candidates(base: &url::Url, html: &str) -> Vec<String> {
    let tags = regex::Regex::new(r"(?is)<(?:link|meta)\b[^>]{0,4096}>").expect("constant");
    let attributes =
        regex::Regex::new(r#"(?is)([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*["']([^"']*)["']"#)
            .expect("constant");
    let mut out = vec![];
    for tag in tags.find_iter(html).take(500) {
        let attrs: std::collections::BTreeMap<_, _> = attributes
            .captures_iter(tag.as_str())
            .map(|c| (c[1].to_ascii_lowercase(), c[2].replace("&amp;", "&")))
            .collect();
        let rel = attrs
            .get("rel")
            .map(|r| r.to_ascii_lowercase())
            .unwrap_or_default();
        let property = attrs
            .get("property")
            .or_else(|| attrs.get("name"))
            .map(String::as_str)
            .unwrap_or("");
        let (rank, raw) = if rel.split_whitespace().any(|p| p == "apple-touch-icon") {
            (0, attrs.get("href"))
        } else if rel
            .split_whitespace()
            .any(|p| p == "icon" || p == "shortcut")
        {
            (1, attrs.get("href"))
        } else if property == "og:image" || property == "twitter:image" {
            (2, attrs.get("content"))
        } else {
            continue;
        };
        if let Some(raw) = raw {
            if let Ok(url) = base.join(raw) {
                if net::validate_url(url.as_str()).is_ok()
                    && policy::safe_read_url(&url)
                    && url.fragment().is_none()
                {
                    out.push((rank, url.to_string()));
                }
            }
        }
    }
    out.sort_by_key(|(rank, _)| *rank);
    let mut unique = std::collections::BTreeSet::new();
    out.into_iter()
        .filter_map(|(_, url)| unique.insert(url.clone()).then_some(url))
        .take(4)
        .collect()
}

fn small_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width <= 4096 && height <= 4096
}
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut offset = 2;
    let mut dimensions = None;
    while offset + 1 < bytes.len() {
        if bytes[offset] != 0xff {
            return None;
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if marker == 0xda {
            return dimensions;
        }
        if marker == 0xd9
            || marker == 0xde
            || [0xc5, 0xc6, 0xc7, 0xcd, 0xce, 0xcf].contains(&marker)
        {
            return None;
        }
        if marker == 0x01 {
            continue;
        }
        let length = u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
        if length < 2 {
            return None;
        }
        let segment = bytes.get(offset..offset.checked_add(length)?)?;
        if [0xc0, 0xc1, 0xc2, 0xc3, 0xc9, 0xca, 0xcb].contains(&marker) {
            if dimensions.is_some()
                || segment.len() < 8
                || segment[7] == 0
                || length != 8 + 3 * segment[7] as usize
            {
                return None;
            }
            let height = u16::from_be_bytes([segment[3], segment[4]]) as u32;
            let width = u16::from_be_bytes([segment[5], segment[6]]) as u32;
            if !small_dimensions(width, height) {
                return None;
            }
            dimensions = Some((width, height));
        }
        offset += length;
    }
    None
}
fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let declared = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    if declared.checked_add(8)? != bytes.len() {
        return None;
    }
    let mut offset = 12;
    let mut canvas = None;
    let mut image = None;
    while offset < bytes.len() {
        let tag = bytes.get(offset..offset + 4)?;
        let length =
            u32::from_le_bytes(bytes.get(offset + 4..offset + 8)?.try_into().ok()?) as usize;
        let end = offset.checked_add(8)?.checked_add(length)?;
        let data = bytes.get(offset + 8..end)?;
        let dimensions = match tag {
            b"VP8X" => {
                if data.len() != 10 || data[0] & 2 != 0 || canvas.is_some() {
                    return None;
                }
                let le24 = |start: usize| {
                    u32::from_le_bytes([data[start], data[start + 1], data[start + 2], 0]) + 1
                };
                canvas = Some((le24(4), le24(7)));
                canvas
            }
            b"VP8 " => {
                if data.len() < 10
                    || data[0] & 1 != 0
                    || &data[3..6] != b"\x9d\x01\x2a"
                    || image.is_some()
                {
                    return None;
                }
                image = Some((
                    (u16::from_le_bytes([data[6], data[7]]) & 0x3fff) as u32,
                    (u16::from_le_bytes([data[8], data[9]]) & 0x3fff) as u32,
                ));
                image
            }
            b"VP8L" => {
                if data.len() < 5 || data[0] != 0x2f || image.is_some() {
                    return None;
                }
                let bits = u32::from_le_bytes(data[1..5].try_into().ok()?);
                if bits >> 29 != 0 {
                    return None;
                }
                image = Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
                image
            }
            b"ANIM" | b"ANMF" => return None,
            _ => None,
        };
        if dimensions.is_some_and(|(width, height)| !small_dimensions(width, height)) {
            return None;
        }
        if length & 1 != 0 && bytes.get(end) != Some(&0) {
            return None;
        }
        offset = end.checked_add(length & 1)?;
    }
    let image = image?;
    if canvas.is_some_and(|canvas| canvas != image) {
        return None;
    }
    Some(image)
}

pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 || bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return (w > 0 && h > 0 && w <= 4096 && h <= 4096).then_some("image/png");
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        return (u16::from_le_bytes([bytes[4], bytes[5]]) <= 32).then_some("image/x-icon");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]);
        let h = u16::from_le_bytes([bytes[8], bytes[9]]);
        return (w > 0 && h > 0 && w <= 4096 && h <= 4096).then_some("image/gif");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return jpeg_dimensions(bytes).map(|_| "image/jpeg");
    }
    if &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return webp_dimensions(bytes).map(|_| "image/webp");
    }
    if bytes.len() <= 256 * 1024 {
        let text = std::str::from_utf8(bytes).ok()?.to_ascii_lowercase();
        if text.contains("<svg")
            && text.contains("</svg>")
            && ![
                "<script",
                "<foreignobject",
                "<iframe",
                "<image",
                "<!doctype",
                "<!entity",
                "@import",
                "javascript:",
                "data:",
                "<animate",
                "<set",
                "<handler",
            ]
            .iter()
            .any(|part| text.contains(part))
        {
            let active = regex::Regex::new(
                r#"\son[a-z]+\s*=|(?:href|src)\s*=\s*["']\s*[^#"']|url\(\s*["']?\s*[^#\s"']"#,
            )
            .expect("constant");
            if !active.is_match(&text) {
                return Some("image/svg+xml");
            }
        }
    }
    None
}
impl Runtime {
    pub(crate) async fn fetch_site_image(
        &self,
        site: &str,
        base: &url::Url,
        mut urls: Vec<String>,
    ) -> AppResult<bool> {
        if !util::valid_id(site) {
            return Err(AppError::api("bad_request", 400));
        }
        // favicon.ico is a conventional browser identity resource, not an API probe.
        if urls.is_empty() {
            urls.push(
                base.join("/favicon.ico")
                    .map_err(|_| AppError::api("bad_url", 400))?
                    .to_string(),
            );
        }
        for url in urls.into_iter().take(3) {
            let response = self
                .network
                .fetch(Request {
                    url: url.clone(),
                    method: "GET".into(),
                    headers: vec![],
                    body: None,
                    account_id: String::new(),
                    limit: 2 * 1024 * 1024,
                    timeout: Duration::from_secs(12),
                    read_only: true,
                    follow_redirects: true,
                    tls_profile: "go".into(),
                })
                .await;
            let Ok(response) = response else {
                continue;
            };
            if !response.successful() {
                continue;
            }
            let Some(kind) = image_type(&response.bytes) else {
                continue;
            };
            util::atomic_file(
                &self.root.join("site-images").join(format!("{site}.image")),
                &response.bytes,
            )?;
            self.state.update(|data| {
                let meta = data.sites.entry(site.into()).or_default();
                meta.icon_type = kind.into();
                meta.icon_source = response.url;
                Ok(())
            })?;
            self.changed();
            return Ok(true);
        }
        Ok(false)
    }
    pub async fn fill_missing_site_images(&self) {
        let Ok(sites) = self.specs() else {
            return;
        };
        for spec in sites {
            let missing = self.state.read().is_ok_and(|data| {
                data.sites
                    .get(&spec.site.name)
                    .is_none_or(|meta| meta.icon_type.is_empty())
            });
            if !missing {
                continue;
            }
            let Ok(base) = net::validate_url(spec.site.source_url()) else {
                continue;
            };
            let urls = match self.read_page(spec.site.source_url(), "").await {
                Ok(page) if page.successful() => {
                    candidates(&base, &String::from_utf8_lossy(&page.bytes))
                }
                _ => continue,
            };
            let _ = self.fetch_site_image(&spec.site.name, &base, urls).await;
        }
    }
    pub fn site_image(&self, site: &str) -> AppResult<(String, Vec<u8>)> {
        if !util::valid_id(site) {
            return Err(AppError::api("not_found", 404));
        }
        let kind = self
            .state
            .read()?
            .sites
            .get(site)
            .map(|m| m.icon_type.clone())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let bytes = util::read_bounded(
            &self.root.join("site-images").join(format!("{site}.image")),
            2 * 1024 * 1024,
        )?;
        if image_type(&bytes) != Some(kind.as_str()) {
            return Err(AppError::api("not_found", 404));
        }
        Ok((kind, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_published_images_are_ranked_without_unsafe_urls() {
        let base = url::Url::parse("https://website.example/docs").unwrap();
        let urls = candidates(
            &base,
            r#"<link rel="icon" href="/favicon.svg"><meta property="og:image" content="https://cdn.example/card.png"><link rel="apple-touch-icon" href="/apple.png"><link rel="icon" href="javascript:alert(1)"><link rel="icon" href="/favicon.svg">"#,
        );
        assert_eq!(
            urls,
            vec![
                "https://website.example/apple.png",
                "https://website.example/favicon.svg",
                "https://cdn.example/card.png"
            ]
        );
    }
    #[test]
    fn compressed_raster_dimensions_are_checked_before_serving() {
        let mut jpeg = vec![
            0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 64, 0, 64, 1, 1, 0x11, 0, 0xff, 0xda,
        ];
        assert_eq!(image_type(&jpeg), Some("image/jpeg"));
        jpeg[9] = 0x7f;
        assert_eq!(image_type(&jpeg), None);
        let webp = |width: u32, height: u32| {
            let bits = (width - 1) | ((height - 1) << 14);
            let mut bytes = b"RIFF".to_vec();
            bytes.extend(18u32.to_le_bytes());
            bytes.extend(b"WEBPVP8L");
            bytes.extend(5u32.to_le_bytes());
            bytes.push(0x2f);
            bytes.extend(bits.to_le_bytes());
            bytes.push(0);
            bytes
        };
        assert_eq!(image_type(&webp(64, 64)), Some("image/webp"));
        assert_eq!(image_type(&webp(5000, 64)), None);
        let mut truncated = webp(64, 64);
        truncated.pop();
        assert_eq!(image_type(&truncated), None);
    }
    #[test]
    fn active_svg_html_and_oversized_images_are_rejected() {
        for svg in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg onload="alert(1)"></svg>"#,
            r#"<svg><image href="https://track.example/pixel"/></svg>"#,
            r#"<svg><style>rect{fill:url(https://track.example/pixel)}</style></svg>"#,
            r#"<html>not an image</html>"#,
        ] {
            assert_eq!(image_type(svg.as_bytes()), None);
        }
        assert_eq!(
            image_type(br##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0L4 4"/></svg>"##),
            Some("image/svg+xml")
        );
        let mut png = vec![0u8; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[16..20].copy_from_slice(&99999u32.to_be_bytes());
        png[20..24].copy_from_slice(&10u32.to_be_bytes());
        assert_eq!(image_type(&png), None);
    }
}
