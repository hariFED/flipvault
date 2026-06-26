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

/// Arcium computation-definition offset: first 4 bytes (LE u32) of sha256("<circuit_name>").
/// Used to derive the comp_def PDA when building MPC queue instructions (keeper/frontend).
pub fn comp_def_offset(circuit: &str) -> u32 {
    let out = Sha256::digest(circuit.as_bytes());
    u32::from_le_bytes([out[0], out[1], out[2], out[3]])
}
