//! Images carried in the IR as `data:` URIs: their bytes, their format (sniffed
//! from the bytes, not the URI's media type) and their natural size (read from
//! the header, in CSS px), for the writers that embed them.
//!
//! Only `data:` URIs are read: an image linked by URL or path is never fetched
//! (the IR may come from anyone, so a writer doesn't read files or the network on
//! its behalf).

use std::str::FromStr;

use crate::length::Length;

/// The image formats writers can embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Svg,
}

impl ImageFormat {
    /// The media type (`image/png`).
    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Svg => "image/svg+xml",
        }
    }

    /// The usual file extension (`png`), without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Gif => "gif",
            ImageFormat::Svg => "svg",
        }
    }
}

/// A browser's size for an image that has none (an SVG without `width`,
/// `height` or `viewBox`): 300×150px.
pub const DEFAULT_SIZE_PX: (f64, f64) = (300.0, 150.0);

/// The largest side [`Image::size_pt`] gives (200in): larger sizes are clamped.
pub const MAX_PT: f64 = 14_400.0;

/// A decoded `data:` URI image.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub format: ImageFormat,
    pub data: Vec<u8>,
    /// The natural size in CSS px (pixels for a bitmap, whose resolution is
    /// ignored as browsers do; `width`/`height` or the `viewBox` for an SVG), if
    /// known.
    pub natural_px: Option<(f64, f64)>,
}

impl Image {
    /// The image a `src` holds: `None` unless it is a `data:` URI (base64 or
    /// percent-encoded) of a PNG, JPEG, GIF or SVG whose header reads and that
    /// is [whole](whole).
    pub fn from_src(src: &str) -> Option<Image> {
        let data = data_uri_bytes(src)?;
        let (format, natural_px) = sniff(&data).filter(|(f, _)| whole(*f, &data))?;
        Some(Image {
            format,
            data,
            natural_px,
        })
    }

    /// The size to draw the image at, in points, from the IR's `width` and
    /// `height` (CSS lengths; `em` and `rem` are relative to `font_pt`) as a
    /// browser sizes an `<img>`: a side that is set (to an absolute length) is
    /// used; a side that isn't keeps the natural aspect ratio, or is the natural
    /// size when neither is set (1px = 0.75pt). `%` and `auto` count as unset.
    /// Sides are clamped to `0..=`[`MAX_PT`].
    pub fn size_pt(&self, width: Option<&str>, height: Option<&str>, font_pt: f64) -> (f64, f64) {
        let set = |v: Option<&str>| {
            v.and_then(|v| Length::from_str(v).ok())
                .and_then(|l| l.to_pt(font_pt, font_pt))
        };
        let (nw, nh) = self.natural_px.unwrap_or(DEFAULT_SIZE_PX);
        let (nw, nh) = (nw * 0.75, nh * 0.75);
        let ratio = self.natural_px.map(|(w, h)| h / w);
        let (w, h) = match (set(width), set(height)) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, ratio.map_or(nh, |r| w * r)),
            (None, Some(h)) => (ratio.map_or(nw, |r| h / r), h),
            (None, None) => (nw, nh),
        };
        let clamp = |v: f64| {
            if v.is_nan() {
                0.0
            } else {
                v.clamp(0.0, MAX_PT)
            }
        };
        (clamp(w), clamp(h))
    }

    /// The image as a base64 `data:` URI of its sniffed media type.
    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.format.mime(), base64(&self.data))
    }
}

/// The bytes of a `data:` URI (`data:[<media type>][;base64],<data>`), or `None`
/// if `src` isn't one or its base64 doesn't decode.
fn data_uri_bytes(src: &str) -> Option<Vec<u8>> {
    let src = src.trim();
    let scheme = src.get(..5)?;
    if !scheme.eq_ignore_ascii_case("data:") {
        return None;
    }
    let (header, payload) = src[5..].split_once(',')?;
    let is_base64 = header
        .rsplit(';')
        .next()
        .is_some_and(|p| p.trim().eq_ignore_ascii_case("base64"));
    let bytes = percent_decode(payload);
    if is_base64 {
        unbase64(&bytes)
    } else {
        Some(bytes)
    }
}

/// `s` with each `%XX` decoded (a `%` not followed by two hex digits is kept).
fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 (standard alphabet, padded).
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode base64 in the standard or URL-safe alphabet, ignoring ASCII whitespace,
/// with or without padding. `None` on any other character or a dangling one.
fn unbase64(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits, mut count) = (0u32, 0u32, 0usize);
    let mut padded = false;
    for &c in s {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                padded = true;
                continue;
            }
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        if padded {
            return None;
        }
        acc = acc << 6 | u32::from(v);
        bits += 6;
        count += 1;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (count % 4 != 1).then_some(out)
}

/// The format and natural size (CSS px) of `data`, or `None` if it isn't a PNG,
/// JPEG, GIF or SVG whose header reads.
fn sniff(data: &[u8]) -> Option<(ImageFormat, Option<(f64, f64)>)> {
    let be16 = |i: usize| Some(u16::from_be_bytes(data.get(i..i + 2)?.try_into().ok()?));
    let be32 = |i: usize| Some(u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?));
    let le16 = |i: usize| Some(u16::from_le_bytes(data.get(i..i + 2)?.try_into().ok()?));
    let size = |w: u32, h: u32| (w > 0 && h > 0).then_some((f64::from(w), f64::from(h)));
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        // The first chunk is IHDR: width and height.
        if data.get(12..16)? != b"IHDR" {
            return None;
        }
        return Some((ImageFormat::Png, Some(size(be32(16)?, be32(20)?)?)));
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        let (w, h) = (le16(6)?, le16(8)?);
        return Some((ImageFormat::Gif, Some(size(u32::from(w), u32::from(h))?)));
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // Walk the markers to the first start-of-frame: height, then width.
        let mut i = 2;
        loop {
            if *data.get(i)? != 0xFF {
                return None;
            }
            let marker = *data.get(i + 1)?;
            match marker {
                0xFF => i += 1,
                0x01 | 0xD0..=0xD7 => i += 2,
                0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                    let (h, w) = (be16(i + 5)?, be16(i + 7)?);
                    return Some((ImageFormat::Jpeg, Some(size(u32::from(w), u32::from(h))?)));
                }
                0xD9 | 0xDA => return None,
                _ => i += 2 + usize::from(be16(i + 2)?),
            }
        }
    }
    svg_size(data).map(|size| (ImageFormat::Svg, size))
}

/// Whether `data`, which [`sniff`] reads as `format`, is whole: not cut short
/// (as a truncated `data:` URI is) nor, for a PNG, corrupt, so that a writer
/// that embeds it (and a reader that decodes it: Typst fails to compile a
/// document with an image it can't) gets a file that holds together. A PNG's
/// chunks run whole, their CRCs matching, from IHDR through image data to
/// IEND; a JPEG's segments from a frame header through a scan to its end
/// marker; a GIF's blocks through an image to its trailer; an SVG closes its
/// root. A file can still hold data that doesn't decode.
fn whole(format: ImageFormat, data: &[u8]) -> bool {
    match format {
        ImageFormat::Png => whole_png(data),
        ImageFormat::Jpeg => whole_jpeg(data),
        ImageFormat::Gif => whole_gif(data),
        ImageFormat::Svg => whole_svg(data),
    }
    .unwrap_or(false)
}

fn whole_png(data: &[u8]) -> Option<bool> {
    let mut i = 8;
    let mut pixels = false;
    loop {
        let len = u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?) as usize;
        let end = i.checked_add(12)?.checked_add(len)?;
        // The CRC covers the type and the data.
        let body = data.get(i + 4..end - 4)?;
        let crc = u32::from_be_bytes(data.get(end - 4..end)?.try_into().ok()?);
        if crc32(body) != crc {
            return Some(false);
        }
        match &body[..4] {
            b"IDAT" => pixels = true,
            b"IEND" => return Some(pixels),
            _ => {}
        }
        i = end;
    }
}

fn whole_jpeg(data: &[u8]) -> Option<bool> {
    let be16 = |i: usize| {
        Some(usize::from(u16::from_be_bytes(
            data.get(i..i + 2)?.try_into().ok()?,
        )))
    };
    let (mut i, mut frame, mut scan) = (2, false, false);
    loop {
        if *data.get(i)? != 0xFF {
            return Some(false);
        }
        match *data.get(i + 1)? {
            // Fill, and markers without a length.
            0xFF => i += 1,
            0x01 | 0xD0..=0xD7 => i += 2,
            0xD9 => return Some(frame && scan),
            0xDA => {
                if !frame {
                    return Some(false);
                }
                // The scan's header, then its coded data, up to the next
                // marker (FF 00 is a coded FF; FF D0-D7 a restart).
                i += 2 + be16(i + 2)?;
                loop {
                    match (*data.get(i)?, *data.get(i + 1)?) {
                        (0xFF, 0x00 | 0xD0..=0xD7) => i += 2,
                        (0xFF, _) => break,
                        _ => i += 1,
                    }
                }
                scan = true;
            }
            m => {
                frame |= matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC);
                i += 2 + be16(i + 2)?;
            }
        }
    }
}

fn whole_gif(data: &[u8]) -> Option<bool> {
    // A color table follows a block whose flags (at `at`) say so.
    let table = |at: usize| -> Option<usize> {
        let flags = *data.get(at)?;
        Some(if flags & 0x80 != 0 {
            3 << ((flags & 7) + 1)
        } else {
            0
        })
    };
    // Data sub-blocks, each its length then its bytes, to a zero length.
    let blocks = |mut i: usize| -> Option<usize> {
        loop {
            let n = usize::from(*data.get(i)?);
            i += 1 + n;
            if n == 0 {
                return Some(i);
            }
        }
    };
    let (mut i, mut image) = (13 + table(10)?, false);
    loop {
        match *data.get(i)? {
            // An extension: its label, then its data.
            0x21 => i = blocks(i + 2)?,
            // An image: its descriptor, color table, code size and data.
            0x2C => {
                i = blocks(i + 10 + table(i + 9)? + 1)?;
                image = true;
            }
            0x3B => return Some(image),
            _ => return Some(false),
        }
    }
}

fn whole_svg(data: &[u8]) -> Option<bool> {
    let text = std::str::from_utf8(data).ok()?.trim_end();
    // The document ends with the root's end tag, or is a root closed by `/>`
    // (and whatever comes before it).
    let closed = text
        .strip_suffix('>')
        .is_some_and(|t| t.trim_end().ends_with("</svg"));
    Some(closed || (text.ends_with("/>") && text.matches('<').count() == 1 + prolog(text)))
}

/// The number of `<` before the root element: an XML declaration, comments
/// and the like (`<?`, `<!`).
fn prolog(text: &str) -> usize {
    text.match_indices('<')
        .take_while(|(i, _)| matches!(text.as_bytes().get(i + 1), Some(b'?' | b'!')))
        .count()
}

/// The CRC-32 a PNG chunk carries (ISO 3309, as zlib's).
fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    };
    !data.iter().fold(!0u32, |c, b| {
        TABLE[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// The natural size (CSS px) of an SVG document: `None` if `data` isn't one,
/// `Some(None)` if it has no size. The root's `width` and `height` (absolute
/// lengths, or unitless px), with a missing one from the `viewBox`'s aspect
/// ratio; or the `viewBox`'s size.
fn svg_size(data: &[u8]) -> Option<Option<(f64, f64)>> {
    let text = std::str::from_utf8(data).ok()?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with('<') {
        return None;
    }
    // The first `<svg` tag outside a comment.
    let mut rest = text;
    let attrs = loop {
        let i = rest.find('<')?;
        rest = &rest[i..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if let Some(after) = rest
            .strip_prefix("<svg")
            .filter(|a| a.starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/'))
        {
            break attributes(after);
        } else {
            rest = &rest[1..];
        }
    };
    let get = |name: &str| attrs.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
    let length = |v: Option<&str>| {
        let v = v?.trim();
        let px = match v.parse::<f64>() {
            Ok(n) => n,
            Err(_) => Length::from_str(v).ok()?.to_pt(12.0, 12.0)? / 0.75,
        };
        (px.is_finite() && px > 0.0).then_some(px)
    };
    let view_box = get("viewBox").and_then(|v| {
        let n: Vec<f64> = v
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().ok())
            .collect::<Option<_>>()?;
        match n[..] {
            [_, _, w, h] if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => Some((w, h)),
            _ => None,
        }
    });
    let size = match (length(get("width")), length(get("height")), view_box) {
        (Some(w), Some(h), _) => Some((w, h)),
        (Some(w), None, Some((vw, vh))) => Some((w, w * vh / vw)),
        (None, Some(h), Some((vw, vh))) => Some((h * vw / vh, h)),
        (None, None, vb) => vb,
        _ => None,
    };
    Some(size)
}

/// The attributes of the start tag that `tag` begins inside (just after its
/// name), up to its `>`: (name, value) pairs.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = tag;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('>') || rest.starts_with('/') {
            return out;
        }
        let name_end = rest
            .find(|c: char| c == '=' || c == '>' || c == '/' || c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = rest[name_end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            if name.is_empty() {
                return out;
            }
            continue;
        };
        let after = after.trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            return out;
        };
        let Some(end) = after[1..].find(quote) else {
            return out;
        };
        out.push((name, &after[1..1 + end]));
        rest = &after[end + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk of a PNG: its length, type, data and CRC.
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let body = [&kind[..], data].concat();
        [
            &(data.len() as u32).to_be_bytes()[..],
            &body,
            &crc32(&body).to_be_bytes(),
        ]
        .concat()
    }

    /// A whole PNG: the signature, IHDR, (stand-in) image data and IEND.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let ihdr = [&w.to_be_bytes()[..], &h.to_be_bytes(), &[8, 6, 0, 0, 0]].concat();
        [
            &b"\x89PNG\r\n\x1a\n"[..],
            &chunk(b"IHDR", &ihdr),
            &chunk(b"IDAT", &[0; 4]),
            &chunk(b"IEND", &[]),
        ]
        .concat()
    }

    fn uri(data: &[u8], mime: &str) -> String {
        format!("data:{mime};base64,{}", base64(data))
    }

    #[test]
    fn base64_round_trips() {
        for n in 0..40 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let enc = base64(&data);
            assert_eq!(enc.len() % 4, 0);
            assert_eq!(unbase64(enc.as_bytes()), Some(data.clone()), "{n}");
            // Unpadded, wrapped and URL-safe forms decode the same.
            let bare = enc.trim_end_matches('=');
            assert_eq!(unbase64(bare.as_bytes()), Some(data.clone()), "{n}");
            let wrapped: String = enc
                .chars()
                .enumerate()
                .flat_map(|(i, c)| (i % 7 == 6).then_some('\n').into_iter().chain([c]))
                .collect();
            assert_eq!(unbase64(wrapped.as_bytes()), Some(data.clone()), "{n}");
            let url = enc.replace('+', "-").replace('/', "_");
            assert_eq!(unbase64(url.as_bytes()), Some(data), "{n}");
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(&[0xFB, 0xFF]), "+/8=");
    }

    #[test]
    fn bad_base64_is_none() {
        for bad in ["TWF*", "T", "TWFuT", "TQ==TQ==", "TQ=a"] {
            assert_eq!(unbase64(bad.as_bytes()), None, "{bad}");
        }
    }

    #[test]
    fn percent_decoding_keeps_stray_percents() {
        assert_eq!(percent_decode("%3Csvg%3e"), b"<svg>");
        assert_eq!(percent_decode("100%"), b"100%");
        assert_eq!(percent_decode("%4"), b"%4");
        assert_eq!(percent_decode("%zz%41"), b"%zzA");
    }

    #[test]
    fn data_uris_are_read_and_others_are_not() {
        let data = png(24, 12);
        let img = Image::from_src(&uri(&data, "image/png")).unwrap();
        assert_eq!(img.format, ImageFormat::Png);
        assert_eq!(img.data, data);
        assert_eq!(img.natural_px, Some((24.0, 12.0)));
        // The format comes from the bytes, not the media type.
        let img = Image::from_src(&uri(&data, "application/octet-stream")).unwrap();
        assert_eq!(img.format, ImageFormat::Png);
        let img = Image::from_src(&format!("  DATA:;BASE64,{}  ", base64(&data))).unwrap();
        assert_eq!(img.natural_px, Some((24.0, 12.0)));
        for src in [
            "logo.png",
            "https://example.com/logo.png",
            "file:///etc/passwd",
            "data:image/png;base64",
            "data:image/png;base64,***",
            "data:text/plain,hello",
            "",
        ] {
            assert_eq!(Image::from_src(src), None, "{src}");
        }
        // A media type parameter after `;base64` means it isn't base64.
        let not_base64 = format!("data:image/png;base64;x=1,{}", base64(&data));
        assert_eq!(Image::from_src(&not_base64), None);
    }

    #[test]
    fn the_example_logo_is_24px_square() {
        // examples.rs's LOGO_PNG.
        let src = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAYUlEQVR42mP8//8/AzUAEwOVAAuEUkt5Q7YRt+aIUNNFw9ggFmICkpgIYSLSFEwusQZh1YbHLCbiTcEvNeLSEZ6YxiXFRJIGPBbg8xqaNvwJkkDKJr54GS1GCAPGQVeLAACHNx+lIoe2YQAAAABJRU5ErkJggg==";
        let img = Image::from_src(src).unwrap();
        assert_eq!(img.natural_px, Some((24.0, 24.0)));
        assert_eq!(img.data_uri(), src);
    }

    #[test]
    fn png_needs_its_ihdr_and_a_size() {
        let mut bad = png(24, 24);
        bad[12..16].copy_from_slice(b"IDAT");
        assert_eq!(sniff(&bad), None);
        assert_eq!(sniff(&png(0, 24)), None);
        assert_eq!(sniff(&png(24, 24)[..20]), None);
    }

    #[test]
    fn gif_and_jpeg_sizes_are_read() {
        let gif = b"GIF89a\x20\x01\x10\x00\x80\x00\x00";
        assert_eq!(sniff(gif), Some((ImageFormat::Gif, Some((288.0, 16.0)))));
        // SOI, an APP0 segment (length 16), a DQT stub (length 4), then SOF2.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        jpeg.extend([0; 14]);
        jpeg.extend([0xFF, 0xFF, 0xDB, 0, 4, 0, 0]);
        jpeg.extend([0xFF, 0xC2, 0, 17, 8, 0x01, 0x2C, 0x02, 0x58, 3]);
        assert_eq!(
            sniff(&jpeg),
            Some((ImageFormat::Jpeg, Some((600.0, 300.0))))
        );
        // A DHT (C4) segment before the frame isn't one.
        let mut dht = vec![0xFF, 0xD8, 0xFF, 0xC4, 0, 6, 0, 0, 0, 0];
        dht.extend([0xFF, 0xC0, 0, 17, 8, 0, 10, 0, 20, 3]);
        assert_eq!(sniff(&dht), Some((ImageFormat::Jpeg, Some((20.0, 10.0)))));
        // No frame before the scan, or a truncated file.
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 2]), None);
        assert_eq!(sniff(&jpeg[..12]), None);
        assert_eq!(sniff(b"GIF89a\x20"), None);
    }

    #[test]
    fn crc32_is_zlibs() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_png_is_whole_with_its_chunks_through_iend() {
        let whole_png = |d: &[u8]| whole(ImageFormat::Png, d);
        let png = png(4, 4);
        assert!(whole_png(&png));
        // Trailing bytes after IEND are ignored.
        assert!(whole_png(&[&png[..], b"junk"].concat()));
        for n in 0..png.len() {
            assert!(!whole_png(&png[..n]), "{n}");
        }
        // A CRC that doesn't match its chunk.
        let mut bad = png.clone();
        bad[33 + 8] ^= 1;
        assert!(!whole_png(&bad));
        // No image data.
        let ihdr = &png[..33];
        assert!(!whole_png(&[ihdr, &chunk(b"IEND", &[])].concat()));
        // Ancillary chunks are skipped.
        let text = chunk(b"tEXt", b"a\0b");
        assert!(whole_png(&[ihdr, &text, &png[33..]].concat()));
        // A length past the end of memory.
        let huge = [&[0xFF; 4][..], b"IDAT"].concat();
        assert!(!whole_png(&[ihdr, &huge].concat()));
    }

    #[test]
    fn a_jpeg_is_whole_with_a_frame_and_a_scan_through_its_end() {
        let whole_jpeg = |parts: &[&[u8]]| whole(ImageFormat::Jpeg, &parts.concat());
        let soi: &[u8] = &[0xFF, 0xD8];
        let app0: &[u8] = &[0xFF, 0xE0, 0, 4, 0, 0];
        let sof: &[u8] = &[0xFF, 0xC0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0];
        let sos: &[u8] = &[0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 0x3F, 0];
        // Coded data with a coded FF, a restart, fill before the end.
        let coded: &[u8] = &[1, 0xFF, 0, 2, 0xFF, 0xD3, 3, 0xFF, 0xFF];
        let eoi: &[u8] = &[0xFF, 0xD9];
        assert!(whole_jpeg(&[soi, app0, sof, sos, coded, eoi]));
        // Progressive: a table between two scans.
        let dht: &[u8] = &[0xFF, 0xC4, 0, 2];
        assert!(whole_jpeg(&[soi, sof, sos, coded, dht, sos, coded, eoi]));
        let all = [soi, app0, sof, sos, coded, eoi].concat();
        for n in 0..all.len() {
            assert!(!whole(ImageFormat::Jpeg, &all[..n]), "{n}");
        }
        // No frame, no scan, a scan before the frame, a second start.
        assert!(!whole_jpeg(&[soi, sos, coded, eoi]));
        assert!(!whole_jpeg(&[soi, sof, eoi]));
        assert!(!whole_jpeg(&[soi, sos, coded, sof, sos, coded, eoi]));
        assert!(!whole_jpeg(&[soi, sof, soi, sos, coded, eoi]));
        // A DHT (C4) segment isn't a frame.
        assert!(!whole_jpeg(&[soi, dht, sos, coded, eoi]));
        // Bytes between segments that aren't a marker.
        assert!(!whole_jpeg(&[soi, &[0], sof, sos, coded, eoi]));
    }

    #[test]
    fn a_gif_is_whole_with_an_image_through_its_trailer() {
        // 1×1, a global table of 2 colors.
        let head: &[u8] = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xFF\xFF\xFF";
        let ext: &[u8] = &[0x21, 0xF9, 4, 0, 0, 0, 0, 0];
        // An image with a local table of 2 colors, code size 2, one block.
        let image: &[u8] = &[
            0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0x80, 0, 0, 0, 1, 1, 1, 2, 2, 0x4C, 0x01, 0,
        ];
        let trailer: &[u8] = &[0x3B];
        let gif = [head, ext, image, trailer].concat();
        assert!(whole(ImageFormat::Gif, &gif));
        for n in 0..gif.len() {
            assert!(!whole(ImageFormat::Gif, &gif[..n]), "{n}");
        }
        let whole_gif = |parts: &[&[u8]]| whole(ImageFormat::Gif, &parts.concat());
        // No image, or an unknown block.
        assert!(!whole_gif(&[head, ext, trailer]));
        assert!(!whole_gif(&[head, &[0x99], image, trailer]));
        // Without a global table.
        let bare = [&head[..10], &[0, 0, 0]].concat();
        assert!(whole_gif(&[&bare, image, trailer]));
    }

    #[test]
    fn an_svg_is_whole_when_its_root_is_closed() {
        let whole_svg = |s: &str| whole(ImageFormat::Svg, s.as_bytes());
        assert!(whole_svg("<svg width='2'><rect/></svg>"));
        assert!(whole_svg("<svg width='2'><rect/></svg >\n"));
        assert!(whole_svg("<svg width='2'/>"));
        assert!(whole_svg("<?xml version='1.0'?><!-- c --><svg width='2'/>"));
        let doc = "<svg width='2'><rect/></svg>";
        for n in 0..doc.len() {
            assert!(!whole_svg(&doc[..n]), "{n}");
        }
        assert!(!whole_svg("<svg width='2'/><rect/>"));
    }

    #[test]
    fn svg_sizes() {
        let size = |s: &str| sniff(s.as_bytes());
        let svg = |n| Some((ImageFormat::Svg, n));
        assert_eq!(
            size(r#"<svg width="20" height="10"/>"#),
            svg(Some((20.0, 10.0)))
        );
        assert_eq!(
            size("\u{feff}<?xml version=\"1.0\"?>\n<!-- <svg width='1'> -->\n<svg xmlns='http://www.w3.org/2000/svg' width='15pt' height = '1in'>"),
            svg(Some((20.0, 96.0)))
        );
        assert_eq!(
            size(r#"<svg viewBox="0 0 40 30" width="80">"#),
            svg(Some((80.0, 60.0)))
        );
        assert_eq!(
            size(r#"<svg viewBox="0,0,40,30" height="15">"#),
            svg(Some((20.0, 15.0)))
        );
        assert_eq!(
            size(r#"<svg viewBox="0 0 40 30">"#),
            svg(Some((40.0, 30.0)))
        );
        // Relative sizes and no viewBox: no natural size.
        assert_eq!(size(r#"<svg width="100%" height="50%">"#), svg(None));
        assert_eq!(size("<svg>"), svg(None));
        assert_eq!(size(r#"<svg width="20">"#), svg(None));
        // Not an SVG.
        assert_eq!(size("<svgx width='1' height='1'>"), None);
        assert_eq!(size("hello <svg>"), None);
        assert_eq!(size("<html></html>"), None);
        let img =
            Image::from_src("data:image/svg+xml,%3Csvg%20width='8'%20height='4'%3E%3C/svg%3E")
                .unwrap();
        assert_eq!(img.natural_px, Some((8.0, 4.0)));
        let img = Image::from_src("data:image/svg+xml;utf8,<svg viewBox='0 0 2 1'/>").unwrap();
        assert_eq!(img.natural_px, Some((2.0, 1.0)));
    }

    #[test]
    fn attributes_read_quotes_and_stop_at_the_tag_end() {
        assert_eq!(
            attributes(r#" a="1" b = 'x>y' c d="2"> e="3""#),
            vec![("a", "1"), ("b", "x>y"), ("d", "2")]
        );
        assert_eq!(attributes(r#" a="1"/>"#), vec![("a", "1")]);
        assert_eq!(attributes(r#" a="1"#), vec![]);
    }

    #[test]
    fn sizes_follow_the_img_rules() {
        let img = Image {
            format: ImageFormat::Png,
            data: vec![],
            natural_px: Some((24.0, 12.0)),
        };
        let size = |w: Option<&str>, h: Option<&str>| img.size_pt(w, h, 10.0);
        assert_eq!(size(None, None), (18.0, 9.0));
        assert_eq!(size(Some("32px"), Some("16px")), (24.0, 12.0));
        assert_eq!(size(Some("40pt"), None), (40.0, 20.0));
        assert_eq!(size(None, Some("2em")), (40.0, 20.0));
        assert_eq!(size(Some("1in"), Some("1pt")), (72.0, 1.0));
        // `%` and `auto` are unset; so is an invalid length.
        assert_eq!(size(Some("50%"), Some("auto")), (18.0, 9.0));
        assert_eq!(size(Some("wide"), Some("10pt")), (20.0, 10.0));
        assert_eq!(size(Some("0"), None), (0.0, 0.0));
        assert_eq!(size(Some("1e300in"), Some("-1pt")), (MAX_PT, 0.0));
        let no_size = Image {
            natural_px: None,
            ..img
        };
        assert_eq!(no_size.size_pt(None, None, 10.0), (225.0, 112.5));
        assert_eq!(no_size.size_pt(Some("10pt"), None, 10.0), (10.0, 112.5));
    }
}
