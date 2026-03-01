fn expr_supported(expr: &crate::eval::expr::Expr) -> bool {
    use crate::eval::expr::Expr;
    use crate::eval::ops::{BinaryOp, UnaryOp};
    match expr {
        Expr::Literal(value) => type_id_for_value(value).is_some(),
        Expr::Name(_) => true,
        Expr::This | Expr::Super => true,
        Expr::Field { target, field: _ } => {
            matches!(target.as_ref(), Expr::Name(_) | Expr::This | Expr::Super)
        }
        Expr::Index { target, indices } => {
            matches!(
                target.as_ref(),
                Expr::Name(_) | Expr::This | Expr::Super
            ) && indices.iter().all(expr_supported)
        }
        Expr::Unary { op, expr } => {
            matches!(op, UnaryOp::Neg | UnaryOp::Not | UnaryOp::Pos) && expr_supported(expr)
        }
        Expr::Binary { op, left, right } => {
            matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::Pow
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Xor
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
            ) && expr_supported(left)
                && expr_supported(right)
        }
        Expr::Call { target, args } => {
            matches!(
                target.as_ref(),
                Expr::Name(_) | Expr::Field { .. }
            ) && args.iter().all(call_arg_supported)
        }
        _ => false,
    }
}

fn call_arg_supported(arg: &crate::eval::CallArg) -> bool {
    use crate::eval::ArgValue;
    match &arg.value {
        ArgValue::Expr(expr) => expr_supported(expr),
        ArgValue::Target(target) => lvalue_supported(target),
    }
}

fn lvalue_supported(target: &crate::eval::expr::LValue) -> bool {
    match target {
        crate::eval::expr::LValue::Name(_) | crate::eval::expr::LValue::Field { .. } => true,
        crate::eval::expr::LValue::Index { indices, .. } => indices.iter().all(expr_supported),
        crate::eval::expr::LValue::Deref(expr) => expr_supported(expr),
    }
}
