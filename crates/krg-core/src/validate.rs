//! Conformance checks (format §11): container, schema, integrity (hashes),
//! structure (bijection/acyclicity/projection consistency), links, media.

use crate::container::Store;
use crate::Result;

/// Run the full conformance check over a document's store.
pub fn validate_document(store: &dyn Store) -> Result<()> {
    unimplemented!("full validation")
}

/// The cheap referential-integrity subset run on every commit.
pub fn validate_integrity(store: &dyn Store) -> Result<()> {
    unimplemented!("integrity subset")
}
