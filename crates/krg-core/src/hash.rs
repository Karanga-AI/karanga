//! Canonical JSON + content hashing (format §9) and `Rev` derivation.
//!
//! Canonical form (§9.1) is obtained by serializing a node to a
//! `serde_json::Value` — whose object keys are a sorted `BTreeMap` — and then
//! to a compact string. Over the integer-only / ASCII-key domain this is
//! byte-identical to RFC 8785, with no external canonicalization dependency.

use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::id::Rev;
use crate::model::Node;
use crate::Result;

/// Canonical serialization of a node part for hashing (§9.1):
/// sorted keys, compact, UTF-8.
pub fn canonicalize(node: &Node) -> Result<String> {
    let value: serde_json::Value =
        serde_json::to_value(node).map_err(|e| Error::Parse(e.to_string()))?;
    serde_json::to_string(&value).map_err(|e| Error::Parse(e.to_string()))
}

/// `"sha256:" + lowercase-hex` over the canonical form (§9).
pub fn content_hash(node: &Node) -> Result<String> {
    let canon = canonicalize(node)?;
    let digest = Sha256::digest(canon.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("sha256:{hex}"))
}

/// Derive the short `Rev` (first 12 hex chars) from a `"sha256:<hex>"` string.
pub fn rev_of(hash: &str) -> Rev {
    let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
    Rev(hex.chars().take(12).collect())
}
