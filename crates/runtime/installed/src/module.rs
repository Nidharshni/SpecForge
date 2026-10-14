use sha2::{Digest, Sha256};
use std::path::Path;

/// The SHA-256 digest of `data`, lowercase hex: what `specforge.lock` pins
/// an installed binary to.
pub fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// An extension binary and its SHA-256 (lowercase hex), computed once: the
/// bytes that are hashed are the bytes that are loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    bytes: Vec<u8>,
    digest: String,
}

impl Module {
    pub fn new(bytes: Vec<u8>) -> Module {
        let digest = hex_sha256(&bytes);
        Module { bytes, digest }
    }

    /// The module the file at `path` holds, read once.
    pub fn read(path: &Path) -> std::io::Result<Module> {
        std::fs::read(path).map(Module::new)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_sha256_in_lowercase_hex() {
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hex_sha256(b"").len(), 64);
    }

    #[test]
    fn a_module_digests_the_bytes_it_holds() {
        let module = Module::new(b"abc".to_vec());
        assert_eq!(module.digest(), hex_sha256(b"abc"));
        assert_eq!(module.bytes(), b"abc");
    }
}
