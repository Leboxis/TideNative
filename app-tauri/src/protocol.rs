use std::sync::Arc;
use tauri::http::{Response, StatusCode, header};
use tidal_cache::CoverCache;

pub const ALLOWED_SIZES: [u16; 5] = [80, 160, 320, 640, 1280];

fn is_allowed_cover_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host_port = rest.split('/').next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("").to_ascii_lowercase();
    host == "tidal.com" || host.ends_with(".tidal.com")
}

pub fn parse_cover_path(path: &str) -> Option<(String, u16)> {
    let trimmed = path.trim_start_matches('/');
    let (raw_id, raw_size) = trimmed.rsplit_once('/')?;
    if raw_id.is_empty() || raw_id.contains("..") {
        return None;
    }
    let size: u16 = raw_size.parse().ok()?;
    if !ALLOWED_SIZES.contains(&size) {
        return None;
    }
    let decoded = percent_decode(raw_id)?;
    if decoded.is_empty() || decoded.contains("..") {
        return None;
    }
    if decoded.contains("://") && !is_allowed_cover_url(&decoded) {
        return None;
    }
    Some((decoded, size))
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub async fn resolve_cover_response(cache: Arc<CoverCache>, path: &str) -> Response<Vec<u8>> {
    let Some((cover_id, size)) = parse_cover_path(path) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(b"invalid cover path".to_vec())
            .unwrap();
    };

    match cache.get_or_fetch(&cover_id, size).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body((*bytes).clone())
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(b"cover not found".to_vec())
            .unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_cover_uri_and_rejects_malformed_paths() {
        assert_eq!(
            parse_cover_path("/abc-123/640"),
            Some(("abc-123".to_string(), 640))
        );
        assert_eq!(
            parse_cover_path("/https%3A%2F%2Fresources.tidal.com%2Fimages%2Faa.jpg/320"),
            Some(("https://resources.tidal.com/images/aa.jpg".to_string(), 320))
        );
        assert_eq!(parse_cover_path("/../../etc/passwd"), None);
        assert_eq!(parse_cover_path("/abc/99999"), None);
        assert_eq!(parse_cover_path("//640"), None);
    }
}
