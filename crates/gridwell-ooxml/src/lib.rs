//! Packaging for the OOXML writers (DOCX, XLSX, PPTX): parts in, zip bytes out.
//!
//! Every package is deterministic: parts are written in the order given, all
//! deflated, all with the same fixed timestamp (1980-01-01, the zip epoch), so the
//! same table always produces byte-identical files. The timestamp is set
//! explicitly rather than left to the zip crate's default, which becomes "now" if
//! anything in the build enables its `time` feature.

use std::io::{Cursor, Write};

use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

pub use zip;

/// Packaging failed. In practice only possible on allocation failure: everything
/// is written to memory.
#[derive(Debug, Error)]
pub enum PackageError {
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("duplicate part name \"{0}\"")]
    DuplicatePart(String),
    #[error("the first part must be [Content_Types].xml, got \"{0}\"")]
    ContentTypesNotFirst(String),
}

/// One part of a package: its path inside the zip and its bytes.
#[derive(Debug, Clone, Copy)]
pub struct Part<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

impl<'a> Part<'a> {
    pub fn new(name: &'a str, data: &'a (impl AsRef<[u8]> + ?Sized)) -> Self {
        Self {
            name,
            data: data.as_ref(),
        }
    }
}

/// Zip `parts`, in order. The first part must be `[Content_Types].xml` (readers
/// don't require it, but the convention lets tools sniff the package type).
pub fn package(parts: &[Part<'_>]) -> Result<Vec<u8>, PackageError> {
    if let Some(first) = parts.first() {
        if first.name != "[Content_Types].xml" {
            return Err(PackageError::ContentTypesNotFirst(first.name.to_string()));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for p in parts {
        if !seen.insert(p.name) {
            return Err(PackageError::DuplicatePart(p.name.to_string()));
        }
    }

    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for p in parts {
        zip.start_file(p.name, options)?;
        zip.write_all(p.data)?;
    }
    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn parts() -> Vec<(String, Vec<u8>)> {
        vec![
            ("[Content_Types].xml".into(), b"<Types/>".to_vec()),
            ("_rels/.rels".into(), b"<Relationships/>".to_vec()),
            (
                "word/document.xml".into(),
                "<w:document>é😀</w:document>".repeat(50).into_bytes(),
            ),
        ]
    }

    fn build(parts: &[(String, Vec<u8>)]) -> Result<Vec<u8>, PackageError> {
        let p: Vec<Part> = parts.iter().map(|(n, d)| Part::new(n, d)).collect();
        package(&p)
    }

    #[test]
    fn round_trips_parts_in_order() {
        let input = parts();
        let bytes = build(&input).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), input.len());
        for (i, (name, data)) in input.iter().enumerate() {
            let mut f = archive.by_index(i).unwrap();
            assert_eq!(f.name(), name);
            assert_eq!(f.compression(), CompressionMethod::Deflated);
            assert_eq!(f.last_modified(), Some(DateTime::default()));
            let mut out = Vec::new();
            f.read_to_end(&mut out).unwrap();
            assert_eq!(&out, data);
        }
    }

    #[test]
    fn is_deterministic() {
        let a = build(&parts()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = build(&parts()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_duplicates_and_misordered_content_types() {
        let mut dup = parts();
        dup.push(dup[1].clone());
        assert!(matches!(build(&dup), Err(PackageError::DuplicatePart(n)) if n == "_rels/.rels"));
        let mut swapped = parts();
        swapped.swap(0, 1);
        assert!(matches!(
            build(&swapped),
            Err(PackageError::ContentTypesNotFirst(_))
        ));
        assert!(build(&[]).is_ok());
    }
}
