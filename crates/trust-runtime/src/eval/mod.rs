//! Evaluator entry point.

#![allow(missing_docs)]

use indexmap::IndexMap;
use smol_str::SmolStr;
use trust_hir::symbols::ParamDirection;
use trust_hir::types::TypeRegistry;
use trust_hir::TypeId;

use crate::error::RuntimeError;
use crate::instance::{create_class_instance, create_fb_instance};
use crate::io::IoAddress;
use crate::memory::{InstanceId, VariableStorage};
use crate::stdlib::{fbs, StandardLibrary};
use crate::value::{default_value_for_type_id, DateTimeProfile, Duration, Value};

pub mod expr;
pub mod ops;
pub mod stmt;

include!("types.rs");
include!("calls.rs");
include!("bindings.rs");
include!("locals.rs");
include!("outputs.rs");
