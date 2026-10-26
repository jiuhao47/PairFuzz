const CLIENT_SEED_DOMAIN: &[u8] = b"diffray-client-seed-v1\0";

#[must_use]
pub(crate) fn derive_client_seed(campaign_seed: u64, stable_index: u32, core_id: u32) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CLIENT_SEED_DOMAIN);
    hasher.update(&campaign_seed.to_le_bytes());
    hasher.update(&stable_index.to_le_bytes());
    hasher.update(&core_id.to_le_bytes());
    let digest = hasher.finalize();
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::derive_client_seed;

    #[test]
    fn same_identity_derives_same_seed() {
        assert_eq!(derive_client_seed(123, 4, 9), derive_client_seed(123, 4, 9));
    }

    #[test]
    fn different_core_or_index_changes_seed() {
        let seed = derive_client_seed(123, 4, 9);
        assert_ne!(seed, derive_client_seed(123, 5, 9));
        assert_ne!(seed, derive_client_seed(123, 4, 10));
    }

    #[test]
    fn seed_derivation_matches_fixed_vector() {
        assert_eq!(
            derive_client_seed(0x0123_4567_89ab_cdef, 7, 11),
            0x8dc1_a5e0_ecc7_4b32
        );
    }
}
