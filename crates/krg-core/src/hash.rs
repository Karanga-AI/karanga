//! Canonical JSON + content hashing (format §9) and `Rev` derivation.
//!
//! The canonicalizer is vendored (~no dependency) and operates over the
//! restricted value domain of §9.1 (integers only, ASCII keys), making it
//! byte-identical to RFC 8785 for Karanga data.

use crate::id::Rev;
use crate::model::Node;

/// Canonical serialization of a node part for hashing (§9.1):
/// sorted keys, compact, UTF-8, integer-only domain.
pub fn canonicalize(part: &Node) -> String {
    unimplemented!("RFC 8785-restricted canonical JSON")
}

/// `"sha256:" + lowercase-hex` over the canonical form (§9).
pub fn content_hash(part: &Node) -> String {
    unimplemented!("sha256 of canonical form")
}

/// Derive the short `Rev` (first 12 hex chars) from a `"sha256:<hex>"` string.
pub fn rev_of(hash: &str) -> Rev {
    let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
    Rev(hex.chars().take(12).collect())
}
