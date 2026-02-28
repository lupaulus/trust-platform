//! OPC UA interoperability profile and IEC type mapping helpers.

#![allow(missing_docs)]

use std::path::Path;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::debug::DebugSnapshot;
use crate::error::RuntimeError;
use crate::value::Value;

#[cfg(feature = "opcua-wire")]
use ::opcua::client::prelude::{AttributeService, ViewService};
#[cfg(feature = "opcua-wire")]
use glob::Pattern;
#[cfg(feature = "opcua-wire")]
use std::collections::HashMap;
#[cfg(feature = "opcua-wire")]
use std::path::PathBuf;
#[cfg(feature = "opcua-wire")]
use std::time::{Duration as StdDuration, Instant};

include!("opcua/contracts.rs");
include!("opcua/mapping.rs");
include!("opcua/wire.rs");

#[cfg(test)]
mod tests;
