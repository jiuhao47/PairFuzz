use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DIFFERENTIAL_SIGNATURE_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DifferentialSignature([u8; 32]);

impl DifferentialSignature {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in self.0 {
            encoded.push(HEX[usize::from(byte >> 4)] as char);
            encoded.push(HEX[usize::from(byte & 0x0f)] as char);
        }
        encoded
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExitClass {
    Ok = 0,
    Crash = 1,
    Timeout = 2,
    Oom = 3,
    Other = 4,
}

impl ExitClass {
    const fn as_byte(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DifferentialSignatureMetadata {
    pub version: u8,
    pub digest: [u8; 32],
}

impl From<DifferentialSignature> for DifferentialSignatureMetadata {
    fn from(signature: DifferentialSignature) -> Self {
        Self {
            version: DIFFERENTIAL_SIGNATURE_VERSION,
            digest: signature.0,
        }
    }
}

libafl_bolts::impl_serdeany!(DifferentialSignatureMetadata);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryElapsedMetadata {
    pub nanos: u64,
}

libafl_bolts::impl_serdeany!(DiscoveryElapsedMetadata);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DifferentialSignatureSetMetadata {
    accepted: HashSet<DifferentialSignature>,
}

impl DifferentialSignatureSetMetadata {
    pub fn insert(&mut self, signature: DifferentialSignature) -> bool {
        self.accepted.insert(signature)
    }

    pub(crate) fn accepted(&self) -> &HashSet<DifferentialSignature> {
        &self.accepted
    }
}

libafl_bolts::impl_serdeany!(DifferentialSignatureSetMetadata);

/// Register metadata before a restarting event manager can deserialize state.
/// With LibAFL's default `serdeany_autoreg` feature these calls are no-ops,
/// while non-autoreg builds perform the required explicit registration.
pub fn register_metadata_types() {
    // SAFETY: client setup calls this once, before any worker thread is started.
    unsafe {
        DifferentialSignatureMetadata::register();
        DiscoveryElapsedMetadata::register();
        DifferentialSignatureSetMetadata::register();
    }
}

fn encode_optional_return(hasher: &mut blake3::Hasher, value: Option<i32>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&value.to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

#[must_use]
pub fn compute_signature(
    exit_a: ExitClass,
    exit_b: ExitClass,
    rv_a: Option<i32>,
    rv_b: Option<i32>,
    map_a: &[u8],
    map_b: &[u8],
) -> DifferentialSignature {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[
        DIFFERENTIAL_SIGNATURE_VERSION,
        exit_a.as_byte(),
        exit_b.as_byte(),
    ]);
    encode_optional_return(&mut hasher, rv_a);
    encode_optional_return(&mut hasher, rv_b);
    hasher.update(blake3::hash(map_a).as_bytes());
    hasher.update(blake3::hash(map_b).as_bytes());
    DifferentialSignature(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        compute_signature, DifferentialSignatureMetadata, DifferentialSignatureSetMetadata,
        DiscoveryElapsedMetadata, ExitClass,
    };

    #[test]
    fn signature_v1_matches_fixed_vector() {
        let signature = compute_signature(
            ExitClass::Crash,
            ExitClass::Timeout,
            Some(-7),
            None,
            &[0, 1, 2, 3],
            &[4, 5, 6],
        );

        assert_eq!(
            signature.as_bytes(),
            &[
                130, 191, 27, 78, 55, 149, 203, 70, 154, 30, 234, 33, 99, 46, 205, 95, 103, 204,
                42, 143, 216, 88, 180, 7, 98, 233, 21, 124, 97, 184, 114, 76,
            ]
        );
    }

    #[test]
    fn signature_distinguishes_none_from_zero() {
        let none = compute_signature(ExitClass::Ok, ExitClass::Ok, None, None, &[1], &[2]);
        let zero = compute_signature(ExitClass::Ok, ExitClass::Ok, Some(0), None, &[1], &[2]);
        assert_ne!(none, zero);
    }

    #[test]
    fn signature_preserves_target_order() {
        let ab = compute_signature(
            ExitClass::Crash,
            ExitClass::Ok,
            Some(1),
            Some(2),
            &[1, 2],
            &[3, 4],
        );
        let ba = compute_signature(
            ExitClass::Ok,
            ExitClass::Crash,
            Some(2),
            Some(1),
            &[3, 4],
            &[1, 2],
        );
        assert_ne!(ab, ba);
    }

    #[test]
    fn signature_changes_with_either_map() {
        let original = compute_signature(
            ExitClass::Ok,
            ExitClass::Ok,
            Some(1),
            Some(2),
            &[1, 2],
            &[3, 4],
        );
        let changed_a = compute_signature(
            ExitClass::Ok,
            ExitClass::Ok,
            Some(1),
            Some(2),
            &[9, 2],
            &[3, 4],
        );
        let changed_b = compute_signature(
            ExitClass::Ok,
            ExitClass::Ok,
            Some(1),
            Some(2),
            &[1, 2],
            &[3, 9],
        );
        assert_ne!(original, changed_a);
        assert_ne!(original, changed_b);
    }

    #[test]
    fn metadata_round_trips_through_serde() {
        let metadata = (
            DifferentialSignatureMetadata {
                version: 1,
                digest: [0x5a; 32],
            },
            DiscoveryElapsedMetadata { nanos: 42 },
        );
        let serialized = serde_json::to_vec(&metadata).unwrap();
        let restored: (DifferentialSignatureMetadata, DiscoveryElapsedMetadata) =
            serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, metadata);
    }

    #[test]
    fn restored_signature_set_rejects_a_local_duplicate() {
        let signature = compute_signature(ExitClass::Ok, ExitClass::Crash, None, None, &[1], &[2]);
        let mut original = DifferentialSignatureSetMetadata::default();
        assert!(original.insert(signature));

        let serialized = serde_json::to_vec(&original).unwrap();
        let mut restored: DifferentialSignatureSetMetadata =
            serde_json::from_slice(&serialized).unwrap();
        assert!(!restored.insert(signature));
    }

    #[test]
    fn signature_metadata_survives_on_disk_sidecar() {
        use libafl::{
            corpus::{Corpus, OnDiskCorpus, Testcase},
            inputs::BytesInput,
            HasMetadata,
        };
        use libafl_bolts::serdeany::SerdeAnyMap;
        use serde::Deserialize;
        use std::{fs, time::SystemTime};

        #[derive(Deserialize)]
        struct StoredSidecar {
            metadata: SerdeAnyMap,
        }

        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "diffray-signature-sidecar-{}-{unique}",
            std::process::id()
        ));
        let signature = super::DifferentialSignature::from_bytes([0xa5; 32]);
        let mut testcase = Testcase::new(BytesInput::new(vec![1, 2, 3]));
        testcase.add_metadata(DifferentialSignatureMetadata::from(signature));
        testcase.add_metadata(DiscoveryElapsedMetadata { nanos: 12_345 });
        let mut corpus = OnDiskCorpus::new(&root).unwrap();
        let id = corpus.add(testcase).unwrap();
        let metadata_path = corpus
            .get(id)
            .unwrap()
            .borrow()
            .metadata_path()
            .clone()
            .unwrap();

        let stored: StoredSidecar =
            serde_json::from_slice(&fs::read(metadata_path).unwrap()).unwrap();
        assert_eq!(
            stored
                .metadata
                .get::<DifferentialSignatureMetadata>()
                .unwrap()
                .digest,
            [0xa5; 32]
        );
        assert_eq!(
            stored
                .metadata
                .get::<DiscoveryElapsedMetadata>()
                .unwrap()
                .nanos,
            12_345
        );
        drop(corpus);
        fs::remove_dir_all(root).unwrap();
    }
}
