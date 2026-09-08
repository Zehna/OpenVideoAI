use sha2::{Digest, Sha256};
use std::io::{self, Read};

/// Streams bytes from an arbitrary reader and computes the SHA-256 hex string and total byte count.
pub fn compute_sha256_reader<R: Read>(reader: &mut R) -> io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total_bytes = 0u64;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total_bytes += n as u64;
    }

    let hash_hex = format!("{:x}", hasher.finalize());
    Ok((hash_hex, total_bytes))
}
