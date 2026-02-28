use indexmap::IndexMap;
use smol_str::SmolStr;

use crate::io::IoAddress;
use crate::task::ProgramDef;
use crate::value::Duration;
use trust_syntax::syntax::{SyntaxKind, SyntaxNode};

use super::super::lower::{const_duration_from_node, const_int_from_node, lower_expr};
use super::super::types::CompileError;
use super::super::util::{
    collect_using_directives, extract_name_from_expr, is_expression_kind, node_text,
};
use super::lower_type_ref;
use super::model::{
    AccessDecl, AccessPart, AccessPath, ConfigInit, ConfigModel, FbTaskBinding, GlobalInit,
    LoweringContext, ProgramInstanceConfig,
};
use super::vars::{parse_var_decl, var_block_kind, var_block_qualifiers, VarBlockKind};

include!("config/entry.rs");
include!("config/globals_access.rs");
include!("config/access_path.rs");
include!("config/tasks_programs.rs");
include!("config/resolve.rs");
