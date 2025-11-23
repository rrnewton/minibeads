use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Hash encoding format for issue IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashEncoding {
    /// Base36 encoding using [0-9a-z] (recommended, matches upstream bd)
    Base36,
    /// Hexadecimal encoding using [0-9a-f] (legacy format)
    Hex,
}

/// Base36 alphabet for encoding (0-9, a-z)
const BASE36_ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Encode bytes to hexadecimal string of specified length (legacy format)
///
/// Converts a byte slice to a hex string representation.
/// This is the legacy encoding format for backwards compatibility.
///
/// # Arguments
/// * `data` - Byte slice to encode
/// * `length` - Desired output length in hex characters
///
/// # Returns
/// A hex string of the specified length
fn encode_hex(data: &[u8], length: usize) -> String {
    // Convert bytes to hex string
    let hex_full = data
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // Take exactly 'length' characters
    if hex_full.len() >= length {
        hex_full[..length].to_string()
    } else {
        // Pad with zeros if needed (shouldn't happen with proper byte count)
        format!("{:0<width$}", hex_full, width = length)
    }
}

/// Encode bytes to base36 string of specified length
///
/// Converts a byte slice to a base36 string representation.
/// Uses big-integer arithmetic to convert bytes to base36.
/// Pads with leading zeros if needed to reach the target length.
///
/// # Arguments
/// * `data` - Byte slice to encode
/// * `length` - Desired output length in base36 characters
///
/// # Returns
/// A base36 string of the specified length
fn encode_base36(data: &[u8], length: usize) -> String {
    use num_bigint::BigUint;
    use num_traits::Zero;

    // Convert bytes to big integer
    let mut num = BigUint::from_bytes_be(data);

    // Handle zero case
    let zero = BigUint::zero();
    if num == zero {
        return "0".repeat(length);
    }

    // Convert to base36
    let base = BigUint::from(36u32);
    let mut chars = Vec::new();

    // Build the string in reverse
    while num > zero {
        let remainder = &num % &base;
        num /= &base;
        // remainder is always 0-35, so this is safe
        let digit_idx = if let Some(digits) = remainder.to_u32_digits().first() {
            *digits as usize
        } else {
            0
        };
        chars.push(BASE36_ALPHABET[digit_idx]);
    }

    // Reverse to get correct order
    chars.reverse();

    // Convert to string
    let mut result = String::from_utf8(chars).unwrap_or_else(|_| String::from("0"));

    // Pad with zeros if needed
    if result.len() < length {
        result = "0".repeat(length - result.len()) + &result;
    }

    // Truncate to exact length if needed (keep least significant digits)
    if result.len() > length {
        result = result[result.len() - length..].to_string();
    }

    result
}

/// Generate a hash-based ID with collision handling
///
/// Takes a collision checker function that returns true if the ID already exists.
/// Uses adaptive length based on estimated database size, and tries multiple nonces
/// before escalating to longer hash lengths.
///
/// # Arguments
/// * `prefix` - The issue prefix (e.g., "minibeads")
/// * `title` - Issue title
/// * `description` - Issue description
/// * `timestamp` - Creation timestamp for deterministic hashing
/// * `estimated_db_size` - Approximate number of existing issues (for adaptive length)
/// * `encoding` - Hash encoding format (base36 or hex)
/// * `collision_check` - Function that returns true if an ID already exists
///
/// # Returns
/// A unique hash-based ID like "minibeads-4f10" or "minibeads-b127a5"
pub fn generate_hash_id_with_collision_check<F>(
    prefix: &str,
    title: &str,
    description: &str,
    timestamp: DateTime<Utc>,
    estimated_db_size: usize,
    encoding: HashEncoding,
    mut collision_check: F,
) -> anyhow::Result<String>
where
    F: FnMut(&str) -> bool,
{
    let creator = "user"; // Default creator

    // Adaptive length based on database size and encoding
    // Base36 starts at length 3 (vs hex which starts at 4)
    let initial_length = match encoding {
        HashEncoding::Base36 => {
            if estimated_db_size < 10 {
                3
            } else if estimated_db_size < 100 {
                4
            } else if estimated_db_size < 1000 {
                5
            } else if estimated_db_size < 10000 {
                6
            } else if estimated_db_size < 100000 {
                7
            } else {
                8
            }
        }
        HashEncoding::Hex => {
            // Hex encoding (legacy): starts at 4 chars
            if estimated_db_size < 100 {
                4
            } else if estimated_db_size < 1000 {
                5
            } else if estimated_db_size < 10000 {
                6
            } else if estimated_db_size < 100000 {
                7
            } else {
                8
            }
        }
    };

    // Try adaptive lengths starting from initial_length, checking for collisions
    for length in initial_length..=8 {
        for nonce in 0..10 {
            let candidate = generate_hash_id(
                prefix,
                title,
                description,
                creator,
                timestamp,
                length,
                nonce,
                encoding,
            );

            // Check for collision using provided function
            if !collision_check(&candidate) {
                return Ok(candidate);
            }
        }
    }

    anyhow::bail!(
        "Failed to generate unique hash ID after trying all lengths and nonces (database has ~{} issues)",
        estimated_db_size
    )
}

/// Generate a hash-based issue ID from content and metadata.
///
/// This matches the upstream beads implementation in internal/storage/sqlite/ids.go
/// The hash is deterministic based on title, description, creator, timestamp, and nonce.
/// Supports both base36 encoding (recommended, matches upstream) and hex encoding (legacy).
///
/// # Arguments
/// * `prefix` - The issue prefix (e.g., "minibeads")
/// * `title` - Issue title
/// * `description` - Issue description
/// * `creator` - Issue creator (typically "user" or system user)
/// * `timestamp` - Creation timestamp
/// * `length` - Number of characters to use (3-8)
/// * `nonce` - Collision avoidance nonce
/// * `encoding` - Hash encoding format (base36 or hex)
///
/// # Returns
/// A hash-based ID like "minibeads-3s9" (base36) or "minibeads-a1b2" (hex)
#[allow(clippy::too_many_arguments)]
pub fn generate_hash_id(
    prefix: &str,
    title: &str,
    description: &str,
    creator: &str,
    timestamp: DateTime<Utc>,
    length: usize,
    nonce: u32,
    encoding: HashEncoding,
) -> String {
    // Combine inputs into stable content string
    // Format matches upstream: "title|description|creator|timestamp_nanos|nonce"
    let content = format!(
        "{}|{}|{}|{}|{}",
        title,
        description,
        creator,
        timestamp.timestamp_nanos_opt().unwrap_or(0),
        nonce
    );

    // Hash with SHA-256
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash_result = hasher.finalize();

    // Encode hash based on selected encoding
    let short_hash = match encoding {
        HashEncoding::Base36 => {
            // Base36 encoding with variable length (3-8 chars)
            // Determine how many bytes to use based on desired output length
            // Matching upstream logic from ids.go generateHashID
            let num_bytes = match length {
                3 => 2, // 2 bytes = 16 bits ≈ 3.09 base36 chars
                4 => 3, // 3 bytes = 24 bits ≈ 4.63 base36 chars
                5 => 4, // 4 bytes = 32 bits ≈ 6.18 base36 chars
                6 => 4, // 4 bytes = 32 bits ≈ 6.18 base36 chars
                7 => 5, // 5 bytes = 40 bits ≈ 7.73 base36 chars
                8 => 5, // 5 bytes = 40 bits ≈ 7.73 base36 chars
                _ => 3, // default to 3 bytes
            };
            encode_base36(&hash_result[..num_bytes], length)
        }
        HashEncoding::Hex => {
            // Hex encoding (legacy): simpler byte-to-length mapping
            // 2 hex chars per byte
            let num_bytes = length.div_ceil(2);
            encode_hex(&hash_result[..num_bytes], length)
        }
    };

    format!("{}-{}", prefix, short_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_generate_hash_id_basic() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();
        let id = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        // Should be format: prefix-hash
        assert!(id.starts_with("test-"));
        assert_eq!(id.len(), "test-".len() + 4); // prefix + dash + 4 base36 chars

        // Verify it's base36 (only contains 0-9, a-z)
        let hash_part = &id["test-".len()..];
        assert!(hash_part
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_hash_id_deterministic() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        let id1 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        let id2 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        // Same inputs should produce same hash
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_hash_id_different_nonce() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        let id1 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        let id2 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            1,
            HashEncoding::Base36,
        );

        // Different nonce should produce different hash
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_hash_id_different_lengths() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        // Base36 encoding supports lengths from 3-8
        for length in 3..=8 {
            let id = generate_hash_id(
                "test",
                "First issue",
                "Test description",
                "user",
                timestamp,
                length,
                0,
                HashEncoding::Base36,
            );

            assert!(id.starts_with("test-"));
            assert_eq!(id.len(), "test-".len() + length);

            // Verify it's base36 (only contains 0-9, a-z)
            let hash_part = &id["test-".len()..];
            assert!(hash_part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_generate_hash_id_different_inputs() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        let id1 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        let id2 = generate_hash_id(
            "test",
            "Second issue", // Different title
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        // Different inputs should produce different hash
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_hash_id_hex_encoding() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();
        let id = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Hex,
        );

        // Should be format: prefix-hash
        assert!(id.starts_with("test-"));
        assert_eq!(id.len(), "test-".len() + 4); // prefix + dash + 4 hex chars

        // Verify it's hex (only contains 0-9, a-f)
        let hash_part = &id["test-".len()..];
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_hash_id_encodings_differ() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        let id_base36 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Base36,
        );

        let id_hex = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
            HashEncoding::Hex,
        );

        // Same inputs with different encodings should produce different IDs
        // (but both should be valid)
        assert_ne!(id_base36, id_hex);
        assert!(id_base36.starts_with("test-"));
        assert!(id_hex.starts_with("test-"));
    }
}
