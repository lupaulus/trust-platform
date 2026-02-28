//! Local package registry contracts and workflows.

#![allow(missing_docs)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::RuntimeBundle;

const REGISTRY_CONFIG_FILE: &str = "registry.toml";
const REGISTRY_INDEX_FILE: &str = "index.json";
const REGISTRY_PACKAGES_DIR: &str = "packages";
const PACKAGE_METADATA_FILE: &str = "metadata.json";
const REGISTRY_SCHEMA_VERSION: u32 = 1;

include!("registry/models.rs");
include!("registry/api_profile.rs");
include!("registry/operations.rs");
include!("registry/config_access.rs");
include!("registry/fs_layout.rs");
include!("registry/digests.rs");
include!("registry/index_metadata.rs");
include!("registry/paths_utils.rs");

#[cfg(test)]
mod tests {
    include!("registry/tests.rs");
}
