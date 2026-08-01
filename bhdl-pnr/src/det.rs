//! Deterministic hash containers: std's SipHash with FIXED keys
//! (`DefaultHasher::default()`), so iteration order is a pure
//! function of the insertion sequence — identical across processes.
//! The per-process RandomState of std's default `HashMap` leaked
//! scheduling-grade nondeterminism into trial outcomes (same input,
//! same seed, different copper run to run).
use std::collections::hash_map::DefaultHasher;
use std::hash::BuildHasherDefault;

pub type HashMap<K, V> =
    std::collections::HashMap<K, V, BuildHasherDefault<DefaultHasher>>;
pub type HashSet<T> =
    std::collections::HashSet<T, BuildHasherDefault<DefaultHasher>>;
