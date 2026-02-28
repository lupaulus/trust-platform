//! Bytecode container format types.

#![allow(missing_docs)]

use smol_str::SmolStr;
use thiserror::Error;

use crate::task::TaskConfig;

include!("format/header.rs");
include!("format/types.rs");
include!("format/refs_consts.rs");
include!("format/pou.rs");
include!("format/resource_io_debug.rs");
include!("format/module.rs");
