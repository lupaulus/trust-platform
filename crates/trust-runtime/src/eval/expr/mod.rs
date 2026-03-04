//! Expression evaluation.

#![allow(missing_docs)]

mod access;
mod ast;
mod call;
mod eval;
mod lvalue;

pub use ast::{Expr, LValue, SizeOfTarget};
pub use eval::eval_expr;
pub use lvalue::{read_lvalue, write_lvalue, write_name};

pub(crate) use call::{
    bind_stdlib_named_args, eval_positional_args, eval_split_call, read_arg_value,
    resolve_instance_method, resolve_using_function,
};
