use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

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
    mut collision_check: F,
) -> anyhow::Result<String>
where
    F: FnMut(&str) -> bool,
{
    let creator = "user"; // Default creator

    // Adaptive length based on database size (matching upstream logic)
    let initial_length = if estimated_db_size < 10 {
        4
    } else if estimated_db_size < 100 {
        5
    } else if estimated_db_size < 1000 {
        6
    } else if estimated_db_size < 10000 {
        7
    } else {
        8
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
/// This matches the upstream beads implementation in internal/storage/sqlite/sqlite.go
/// The hash is deterministic based on title, description, creator, timestamp, and nonce.
///
/// # Arguments
/// * `prefix` - The issue prefix (e.g., "minibeads")
/// * `title` - Issue title
/// * `description` - Issue description
/// * `creator` - Issue creator (typically "user" or system user)
/// * `timestamp` - Creation timestamp
/// * `length` - Number of hex characters to use (4-8)
/// * `nonce` - Collision avoidance nonce
///
/// # Returns
/// A hash-based ID like "minibeads-4f10" or "minibeads-b127a5"
pub fn generate_hash_id(
    prefix: &str,
    title: &str,
    description: &str,
    creator: &str,
    timestamp: DateTime<Utc>,
    length: usize,
    nonce: u32,
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

    // Extract variable-length prefix (4-8 hex chars)
    let short_hash = match length {
        4 => {
            // 2 bytes → 4 hex chars
            format!("{:02x}{:02x}", hash_result[0], hash_result[1])
        }
        5 => {
            // 2.5 bytes → 5 hex chars (take first 5 chars from 3 bytes)
            let three_byte_hex = format!(
                "{:02x}{:02x}{:02x}",
                hash_result[0], hash_result[1], hash_result[2]
            );
            three_byte_hex[..5].to_string()
        }
        6 => {
            // 3 bytes → 6 hex chars
            format!(
                "{:02x}{:02x}{:02x}",
                hash_result[0], hash_result[1], hash_result[2]
            )
        }
        7 => {
            // 3.5 bytes → 7 hex chars (take first 7 chars from 4 bytes)
            let four_byte_hex = format!(
                "{:02x}{:02x}{:02x}{:02x}",
                hash_result[0], hash_result[1], hash_result[2], hash_result[3]
            );
            four_byte_hex[..7].to_string()
        }
        8 => {
            // 4 bytes → 8 hex chars
            format!(
                "{:02x}{:02x}{:02x}{:02x}",
                hash_result[0], hash_result[1], hash_result[2], hash_result[3]
            )
        }
        _ => {
            // Default to 6 hex chars (3 bytes)
            format!(
                "{:02x}{:02x}{:02x}",
                hash_result[0], hash_result[1], hash_result[2]
            )
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
        );

        // Should be format: prefix-hash
        assert!(id.starts_with("test-"));
        assert_eq!(id.len(), "test-".len() + 4); // prefix + dash + 4 hex chars
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
        );

        let id2 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            0,
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
        );

        let id2 = generate_hash_id(
            "test",
            "First issue",
            "Test description",
            "user",
            timestamp,
            4,
            1,
        );

        // Different nonce should produce different hash
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_hash_id_different_lengths() {
        let timestamp = Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap();

        for length in 4..=8 {
            let id = generate_hash_id(
                "test",
                "First issue",
                "Test description",
                "user",
                timestamp,
                length,
                0,
            );

            assert!(id.starts_with("test-"));
            assert_eq!(id.len(), "test-".len() + length);
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
        );

        let id2 = generate_hash_id(
            "test",
            "Second issue", // Different title
            "Test description",
            "user",
            timestamp,
            4,
            0,
        );

        // Different inputs should produce different hash
        assert_ne!(id1, id2);
    }
}
