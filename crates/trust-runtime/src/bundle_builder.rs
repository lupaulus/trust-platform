//! Bundle build helpers (compile sources to program.stbc).

use anyhow::Context;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::harness::{CompileSession, SourceFile};

include!("bundle_builder/contracts.rs");
include!("bundle_builder/build.rs");
include!("bundle_builder/deps.rs");

#[cfg(test)]
mod tests {
    include!("bundle_builder/tests.rs");
}
