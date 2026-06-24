use sha2::{Digest, Sha256};

/// Anchor instruction discriminator: first 8 bytes of sha256("global:<name>").
pub fn discriminator(name: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(b"global:");
    h.update(name.as_bytes());
    let out = h.finalize();
    let mut d = [0u8; 8];
    d.copy_from_slice(&out[..8]);
    d
}

/// Anchor account discriminator: first 8 bytes of sha256("account:<StructName>").
pub fn account_discriminator(name: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(b"account:");
    h.update(name.as_bytes());
    let out = h.finalize();
    let mut d = [0u8; 8];
    d.copy_from_slice(&out[..8]);
    d
}
