//! Deterministic IDs for locally-created resources.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use uuid::Uuid;

/// Builds a compact deterministic resource ID from the resource name and local path.
pub fn stable_resource_id(prefix: &str, name: &str, path: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(name.as_bytes());
    digest.update(b"\0");
    digest.update(path.as_bytes());
    let hash = digest.finalize();

    let mut id = String::with_capacity(prefix.len() + 17);
    id.push_str(prefix);
    id.push('-');
    for byte in &hash[..8] {
        write!(&mut id, "{byte:02x}").expect("writing to a string cannot fail");
    }
    id
}

/// Builds a deterministic UUID from the resource name and local path.
pub fn stable_resource_uuid(name: &str, path: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(name.as_bytes());
    digest.update(b"\0");
    digest.update(path.as_bytes());
    let hash = digest.finalize();

    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_resource_ids_use_sha256_digest_prefix() {
        assert_eq!(
            stable_resource_id("PREFIX", "Name", "path/file.py"),
            "PREFIX-a563c77770234e1b"
        );
    }

    #[test]
    fn stable_resource_uuids_use_sha256_digest_prefix() {
        assert_eq!(
            stable_resource_uuid("Name", "path/file.py"),
            "a563c777-7023-8e1b-b6fc-16aa0a938305"
        );
    }
}
