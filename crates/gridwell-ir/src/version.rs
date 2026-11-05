//! IR versions: `ir_version` is `MAJOR.MINOR`.
//!
//! This gridwell reads IR [`IR_VERSION`]. A table of another major version is
//! refused (`IR_VERSION` validation error; a parse error when the JSON doesn't
//! parse as this major's IR). A table of a newer minor version renders, with a
//! warning: what the newer minors added is ignored.

use std::fmt;

/// The IR version this gridwell reads, and writes.
pub const IR_VERSION: &str = "1.0";

/// A well-formed `ir_version`: `MAJOR.MINOR`, each ASCII digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IrVersion {
    pub major: u32,
    pub minor: u32,
}

impl IrVersion {
    /// The IR version this gridwell reads ([`IR_VERSION`]).
    pub const CURRENT: IrVersion = IrVersion { major: 1, minor: 0 };

    /// Parse `MAJOR.MINOR` (`"1.0"`; leading zeros allowed, nothing around it).
    /// A part too large for a `u32` saturates: it's still a version, and not one
    /// this gridwell reads.
    pub fn parse(s: &str) -> Option<IrVersion> {
        let (major, minor) = s.split_once('.')?;
        Some(IrVersion {
            major: number(major)?,
            minor: number(minor)?,
        })
    }

    /// Whether this gridwell reads tables of this version: its major version.
    pub fn is_read(self) -> bool {
        self.major == Self::CURRENT.major
    }

    /// Whether this is a minor version newer than this gridwell's: it reads the
    /// table, ignoring what the newer minors added.
    pub fn is_newer_minor(self) -> bool {
        self.is_read() && self.minor > Self::CURRENT.minor
    }
}

fn number(digits: &str) -> Option<u32> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(digits.bytes().fold(0u32, |n, b| {
        n.saturating_mul(10).saturating_add(u32::from(b - b'0'))
    }))
}

impl fmt::Display for IrVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Why this gridwell doesn't read a table's `ir_version` (`found`, the value as
/// the table has it).
pub(crate) fn version_problem(found: &str) -> Option<String> {
    match IrVersion::parse(found) {
        None => Some(format!(
            "ir_version is {}, not MAJOR.MINOR (this gridwell reads \"{IR_VERSION}\")",
            quoted(found)
        )),
        Some(v) if !v.is_read() => Some(unknown_major(&quoted(found), v)),
        Some(_) => None,
    }
}

/// The message for a table of a major version this gridwell doesn't read.
pub(crate) fn unknown_major(shown: &str, v: IrVersion) -> String {
    let current = IrVersion::CURRENT.major;
    format!(
        "ir_version is {shown}, IR {}, and this gridwell reads only IR {current}.x{}",
        v.major,
        if v.major > current {
            " (a newer gridwell may read it)"
        } else {
            ""
        }
    )
}

/// `s` as a JSON string.
fn quoted(s: &str) -> String {
    serde_json::Value::from(s).to_string()
}
