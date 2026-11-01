//! Generic functions and types: `function first[T](items: T[]): Option[T]`,
//! `type Pair[A, B] = { first: A, second: B }`.
//!
//! In a signature, the type parameters are `Type::Param(i)`, replaced at each call by the
//! types inferred from the arguments (and from the expected type). Inside the body, they are
//! opaque types (`TypeKind::Param`): values of type `T` can be stored, passed, returned and
//! printed, but have no fields and no operators, as type parameters have no constraints.

use super::*;

impl Checker {
    /// `Type::Param(0..n)`, for a signature.
    pub(super) fn signature_params(count: usize) -> Vec<Type> {
        (0..count).map(Type::Param).collect()
    }

    /// New opaque types named after `names`, for a body.
    pub(super) fn opaque_params(&mut self, names: &[String]) -> Vec<Type> {
        names
            .iter()
            .map(|name| Self::named(self.table.add(name, 0, TypeKind::Param)))
            .collect()
    }

    /// Runs `f` with the type names `names` meaning `types`.
    pub(super) fn with_type_params<R>(
        &mut self,
        names: &[String],
        types: &[Type],
        f: impl FnOnce(&mut Self) -> CheckResult<R>,
    ) -> CheckResult<R> {
        if names.is_empty() {
            return f(self);
        }
        self.push_scope();
        for (name, ty) in names.iter().zip(types) {
            self.current_scope().types.insert(name.clone(), TypeRef::Type(ty.clone()));
        }
        let result = f(self);
        self.pop_scope();
        result
    }

    pub(super) fn is_type_param(&self, ty: &Type) -> bool {
        matches!(ty, Type::Named { id, .. } if matches!(self.table.get(*id).kind, TypeKind::Param))
    }

    /// Learns the type parameters of `pattern` (a signature type) from `actual`.
    pub(super) fn unify(&self, pattern: &Type, actual: &Type, generic: &mut Vec<Type>) {
        match (pattern, actual) {
            (Type::Param(i), actual) => {
                if let Some(slot) = generic.get_mut(*i) {
                    if slot.contains_unknown() {
                        *slot = self.join(slot, actual).unwrap_or_else(|| actual.clone());
                    }
                }
            }
            (Type::List(pattern), Type::List(actual)) => self.unify(pattern, actual, generic),
            (Type::Named { id, args }, Type::Named { id: actual_id, args: actual_args })
            | (Type::Named { id, args }, Type::Variant { id: actual_id, args: actual_args, .. })
                if id == actual_id =>
            {
                for (pattern, actual) in args.iter().zip(actual_args) {
                    self.unify(pattern, actual, generic);
                }
            }
            (
                Type::Function { params, ret },
                Type::Function {
                    params: actual_params,
                    ret: actual_ret,
                },
            ) => {
                for (pattern, actual) in params.iter().zip(actual_params) {
                    self.unify(pattern, actual, generic);
                }
                self.unify(ret, actual_ret, generic);
            }
            _ => {}
        }
    }

    /// Type parameters cannot stand for move-only values yet: a generic function could copy
    /// them.
    pub(super) fn ensure_copyable_args(&self, args: &[Type]) -> CheckResult<()> {
        for arg in args {
            if self.is_move_only(arg) {
                return self.err(format!(
                    "a type parameter cannot be {}: type parameters cannot be move-only values yet",
                    self.show(arg)
                ));
            }
        }
        Ok(())
    }

    /// A call to a generic function whose signature is `signature`, with `generic` holding what
    /// is already known of its type parameters (those of the type, for a method).
    pub(super) fn check_generic_call(
        &mut self,
        signature: &Type,
        mut generic: Vec<Type>,
        modes: &[ParamMode],
        args: &[AstNode],
        expected: Option<&Type>,
    ) -> CheckResult<Type> {
        let (params, ret) = match signature {
            Type::Function { params, ret } => (params, ret),
            _ => return self.err("a value of this type cannot be called".to_string()),
        };
        if params.len() != args.len() {
            return self.err(format!(
                "expected {} argument(s), found {}",
                params.len(),
                args.len()
            ));
        }
        // `const nothing: Option[int] = first([])`: the expected type tells `T`.
        if let Some(expected) = expected {
            self.unify(ret, expected, &mut generic);
        }
        for (i, (param, arg)) in params.iter().zip(args).enumerate() {
            let mode = modes.get(i).copied().unwrap_or(ParamMode::Borrow);
            let wanted = param.substitute(&generic);
            let actual = match (arg.unlocated(), mode) {
                (AstNode::MutArg(place), ParamMode::Mut) => self.check_mut_place(place, None)?,
                (AstNode::MutArg(_), _) => {
                    return self.err(format!("argument {} is not a 'mut' parameter: remove 'mut'", i + 1))
                }
                (_, ParamMode::Mut) => {
                    return self.err(format!(
                        "argument {} is a 'mut' parameter: pass a variable with 'mut' ('f(mut x)')",
                        i + 1
                    ))
                }
                _ => self.check_value(arg, Some(&wanted))?,
            };
            self.unify(param, &actual, &mut generic);
            let wanted = param.substitute(&generic);
            if !self.assignable(&wanted, &actual) {
                return self.err(format!(
                    "argument {} must be {}, found {}",
                    i + 1,
                    self.show(&wanted),
                    self.show(&actual)
                ));
            }
            match mode {
                ParamMode::Take => self.consume(arg, &actual)?,
                ParamMode::Borrow => self.note_temporary(arg, &actual),
                ParamMode::Mut => {}
            }
        }
        self.ensure_copyable_args(&generic)?;
        Ok(ret.substitute(&generic))
    }
}
