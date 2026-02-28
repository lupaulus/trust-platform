fn expr_supported(expr: &crate::eval::expr::Expr) -> bool {
    use crate::eval::expr::Expr;
    use crate::eval::ops::{BinaryOp, UnaryOp};
    match expr {
        Expr::Literal(value) => {
            if matches!(value, Value::String(_) | Value::WString(_)) {
                return false;
            }
            type_id_for_value(value).is_some()
        }
        Expr::Name(_) => true,
        Expr::Field { target, field: _ } => matches!(target.as_ref(), Expr::Name(_)),
        Expr::Index { target, indices } => {
            matches!(target.as_ref(), Expr::Name(_)) && indices.iter().all(expr_supported)
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
        _ => false,
    }
}
