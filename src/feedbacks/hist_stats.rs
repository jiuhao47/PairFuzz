use libafl::{Error, HasNamedMetadata};
use libafl_bolts::impl_serdeany;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;

/// Historical value variant for single-key history: either an RV value (Option<i32>)
/// or a PL count (usize).
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum HistVal {
    Rv(Option<i32>),
    Pl(usize),
    /// exact indices vector for ComboCov semantics
    Cov(Vec<usize>),
}

/// Generalized historical-key type: either a single (observer_name, value)
/// or a combo vector of values.
pub enum HistKey {
    Single(String, HistVal),
    Combo(Vec<HistVal>),
}

/// Historical stats for single items: store (observer name, HistVal)
/// so we can treat RV and PL uniformly without matching at call sites.
pub type SingleSeenStats = SingleHistStats<(String, HistVal)>;
impl_serdeany!(SingleSeenStats);

/// General combo history stats for Vec<HistVal>
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComboSeenStats {
    pub seen: HashSet<SmallVec<[HistVal; 4]>>,
}
impl_serdeany!(ComboSeenStats);

/// Check whether a given key is present in historical metadata stored under `stats_name`.
pub fn history_contains<S: HasNamedMetadata>(state: &S, stats_name: &str, key: &HistKey) -> bool {
    // cache named metadata map to avoid repeated lookups
    let named = state.named_metadata_map();
    match key {
        HistKey::Single(name, val) => {
            if let Some(md) = named.get::<SingleSeenStats>(stats_name) {
                md.seen.contains(&(name.clone(), val.clone()))
            } else {
                false
            }
        }
        HistKey::Combo(vec) => {
            if let Some(md) = named.get::<ComboSeenStats>(stats_name) {
                // HashSet<SmallVec> implements Borrow<[HistVal]>, so pass a slice
                md.seen.contains(vec.as_slice())
            } else {
                false
            }
        }
    }
}

/// Insert a key into the historical metadata set stored under `stats_name`.
pub fn history_insert<S: HasNamedMetadata>(
    state: &mut S,
    stats_name: &str,
    key: &HistKey,
) -> Result<(), Error> {
    // cache maps to avoid repeated lookups
    match key {
        HistKey::Single(name, val) => {
            let named_map = state.named_metadata_map();
            if named_map.get::<SingleSeenStats>(stats_name).is_none() {
                state.add_named_metadata_checked(
                    stats_name,
                    SingleSeenStats {
                        seen: std::collections::HashSet::new(),
                    },
                )?;
            }
            let stats_mut = state
                .named_metadata_map_mut()
                .get_mut::<SingleSeenStats>(stats_name)
                .ok_or_else(|| {
                    Error::illegal_argument("history_insert: single stats missing after create")
                })?;
            stats_mut.seen.insert((name.clone(), val.clone()));
            Ok(())
        }
        HistKey::Combo(vec) => {
            let named_map = state.named_metadata_map();
            if named_map.get::<ComboSeenStats>(stats_name).is_none() {
                state.add_named_metadata_checked(
                    stats_name,
                    ComboSeenStats {
                        seen: std::collections::HashSet::new(),
                    },
                )?;
            }
            let stats_mut = state
                .named_metadata_map_mut()
                .get_mut::<ComboSeenStats>(stats_name)
                .ok_or_else(|| {
                    Error::illegal_argument("history_insert: combo stats missing after create")
                })?;
            // Convert Vec<HistVal> -> SmallVec for storage to reduce allocations
            stats_mut.seen.insert(vec.clone().into());
            Ok(())
        }
    }
}

/// Generic single-value historical stats container.
///
/// Stores a HashSet of observed values of type `V` and can be used to
/// create concrete historical stats types such as ReturnValueSingleStats
/// or SinglePlStats via a type alias.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound = "V: Serialize + DeserializeOwned + Eq + Hash + Clone + std::fmt::Debug")]
pub struct SingleHistStats<V>
where
    V: Serialize + DeserializeOwned + Eq + Hash + Clone + std::fmt::Debug,
{
    pub seen: HashSet<V>,
}

impl<V> Default for SingleHistStats<V>
where
    V: Serialize + for<'de> Deserialize<'de> + Eq + Hash + Clone + std::fmt::Debug,
{
    fn default() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }
}

use std::collections::hash_map::DefaultHasher;

/// Produce a small fingerprint (u64) for a HistKey so we can log a compact
/// representation instead of long vectors. We use the default hasher for
/// deterministic-ish output within a run; the hex string is easier to scan.
pub fn fingerprint(key: &HistKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    match key {
        HistKey::Single(name, val) => {
            0u8.hash(&mut hasher);
            name.hash(&mut hasher);
            val.hash(&mut hasher);
        }
        HistKey::Combo(vec) => {
            1u8.hash(&mut hasher);
            vec.len().hash(&mut hasher);
            for v in vec.iter() {
                v.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}
