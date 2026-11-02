//! Function bodies: parameters and return types.

use super::*;

impl Checker {
    pub(super) fn check_function(
        &mut self,
        name: &str,
        params: &[Param],
        signature: Type,
        body: &AstNode,
    ) -> CheckResult<Type> {
        let (param_types, declared_ret) = match signature {
            Type::Function { params, ret } => (params, *ret),
            _ => unreachable!("function signature is always a function type"),
        };
        let declared = if declared_ret == Type::Unknown {
            None
        } else {
            Some(declared_ret.clone())
        };

        self.push_scope();
        self.state.function_scopes.push(self.state.scopes.len() - 1);
        for (param, ty) in params.iter().zip(&param_types) {
            if (param.is_mut || param.is_take) && name == "anonymous function" {
                return self.err(format!(
                    "anonymous functions cannot have '{}' parameters ('{}')",
                    if param.is_mut { "mut" } else { "take" },
                    param.name
                ));
            }
            // Only `mut` parameters (and `mut self`) can be modified; the others are immut.
            let mut symbol = Symbol::value(ty.clone(), !param.is_mut);
            symbol.is_imut = !param.is_mut;
            symbol.reason = Some("parameters can only be modified when declared 'mut'");
            // A `take` parameter owns its value; the others only borrow it.
            if !param.is_take {
                symbol.borrowed = Some(if param.name == "self" { "'self'" } else { "a parameter" });
            }
            self.declare(&param.name, symbol)?;
        }
        // Moves inside the function only concern its own variables.
        let saved_moved = self.state.moved.clone();
        let saved_loops = std::mem::take(&mut self.state.loop_scopes);
        self.state.returns.push(ReturnContext {
            declared: declared.clone(),
            inferred: None,
        });

        let value_expected = declared.as_ref().filter(|t| **t != Type::Void);
        let body_result = match body.unlocated() {
            AstNode::Block(statements) => self.check_statements(statements, value_expected),
            expression => self.check_value(expression, value_expected),
        }
        .and_then(|body_ty| {
            // The value of the last expression is returned (moved out), or dropped when the
            // function returns nothing.
            let tail = match body.unlocated() {
                AstNode::Block(statements) => statements.last().filter(|s| !is_statement(s)),
                expression => Some(expression),
            };
            if let Some(tail) = tail {
                if declared == Some(Type::Void) {
                    self.note_temporary(tail, &body_ty);
                } else {
                    self.consume(tail, &body_ty)?;
                }
            }
            Ok(body_ty)
        });
        let context = self.state.returns.pop();
        self.state.function_scopes.pop();
        self.pop_scope();
        self.state.moved = saved_moved;
        self.state.loop_scopes = saved_loops;
        let body_ty = body_result?;

        let ret = match declared {
            Some(Type::Void) => Type::Void,
            Some(declared) => {
                let ends_with_expression = match body.unlocated() {
                    AstNode::Block(statements) => {
                        statements.last().is_some_and(|s| !is_statement(s))
                    }
                    _ => true,
                };
                if body_ty == Type::Never {
                    // Every path returns explicitly.
                } else if !ends_with_expression || body_ty == Type::Void {
                    return self.err(format!(
                        "'{}' must return a value of type {} (add a 'return' or end with an expression)",
                        name,
                        self.show(&declared)
                    ));
                } else if !self.assignable(&declared, &body_ty) {
                    return self.err(format!(
                        "'{}' returns {}, but its declared return type is {}",
                        name,
                        self.show(&body_ty),
                        self.show(&declared)
                    ));
                }
                declared
            }
            None => {
                let inferred = context.and_then(|c| c.inferred);
                let is_block = matches!(body.unlocated(), AstNode::Block(_));
                match (inferred, body_ty) {
                    (Some(inferred), Type::Never) => inferred,
                    (Some(inferred), tail) if is_block && !matches!(tail, Type::Void) => {
                        self.join(&inferred, &tail).unwrap_or(inferred)
                    }
                    (Some(inferred), _) => inferred,
                    (None, Type::Never) => Type::Void,
                    (None, tail) => tail,
                }
            }
        };
        Ok(Type::Function {
            params: param_types,
            ret: Box::new(ret),
        })
    }
}
