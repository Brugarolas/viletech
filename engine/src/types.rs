//! Small types and type aliases that don't belong anywhere else.

use std::hash::BuildHasherDefault;

use dashmap::DashMap;
use indexmap::IndexMap;
use rustc_hash::FxHasher;

pub type FxDashMap<K, V> = DashMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxIndexMap<K, V> = IndexMap<K, V, BuildHasherDefault<FxHasher>>;
