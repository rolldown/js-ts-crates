mod import_export;
mod package_json;
mod protocols;

pub use import_export::*;
pub use package_json::*;
pub use protocols::*;
pub use semver::{Version, VersionReq};

use indexmap::IndexMap;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;

pub(crate) type FxIndexMap<K, V> = IndexMap<K, V, BuildHasherDefault<FxHasher>>;
