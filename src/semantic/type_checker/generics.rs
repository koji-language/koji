//! Generic functions and types: `function first[T](items: T[]): Option[T]`,
//! `type Pair[A, B] = { first: A, second: B }`.
//!
//! In a signature, the type parameters are `Type::Param(i)`, replaced at each call by the
//! types inferred from the arguments (and from the expected type). Inside the body, they are
//! opaque types (`TypeKind::Param`): values of type `T` can be stored, passed, returned and
//! printed, but have no fields and no operators, as type parameters have no constraints.

use super::*;

/// A generic function being called: its signature (with `Type::Param`), what is already known
/// of its type parameters (those of the type, for a method), their constraints, how each
/// parameter is passed, and its name for messages.
pub(super) struct GenericCallee<'a> {
    pub signature: &'a Type,
    pub generic: Vec<Type>,
    pub constraints: &'a [Constraint],
    pub modes: &'a [ParamMode],
    pub name: &'a str,
}

impl Checker {
    /// `Type::Param(0..n)`, for a signature.
    pub(super) fn signature_params(count: usize) -> Vec<Type> {
        (0..count).map(Type::Param).collect()
    }

    /// The constraint of each type parameter.
    pub(super) fn constraints(&self, params: &[TypeParam]) -> CheckResult<Vec<Constraint>> {
        params
            .iter()
            .map(|param| match param.constraint.as_deref() {
                None => Ok(Constraint::None),
                Some("equatable") => Ok(Constraint::Equatable),
                Some("orderable") => Ok(Constraint::Orderable),
                Some(other) => self.err(format!(
                    "unknown constraint '{}' for '{}': the constraints are 'equatable' and 'orderable'",
                    other, param.name
                )),
            })
            .collect()
    }

    /// New opaque types for `params`, for a body.
    pub(super) fn opaque_params(&mut self, params: &[TypeParam]) -> CheckResult<Vec<Type>> {
        let constraints = self.constraints(params)?;
        Ok(params
            .iter()
            .zip(constraints)
            .map(|(param, constraint)| Self::named(self.table.add(&param.name, 0, TypeKind::Param(constraint))))
            .collect())
    }

    /// Runs `f` with the names of `params` meaning `types`.
    pub(super) fn with_type_params<R>(
        &mut self,
        params: &[TypeParam],
        types: &[Type],
        f: impl FnOnce(&mut Self) -> CheckResult<R>,
    ) -> CheckResult<R> {
        if params.is_empty() {
            return f(self);
        }
        self.push_scope();
        for (param, ty) in params.iter().zip(types) {
            self.current_scope().types.insert(param.name.clone(), TypeRef::Type(ty.clone()));
        }
        let result = f(self);
        self.pop_scope();
        result
    }

    pub(super) fn is_type_param(&self, ty: &Type) -> bool {
        matches!(ty, Type::Named { id, .. } if matches!(self.table.get(*id).kind, TypeKind::Param(_)))
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

    /// Whether `ty` meets `constraint`. The compiler decides from the structure of the type: a
    /// type is equatable when everything it holds is (no functions, no move-only values), and
    /// orderable when it is a number or a string.
    pub(super) fn satisfies(&self, ty: &Type, constraint: Constraint) -> bool {
        match constraint {
            Constraint::None => true,
            Constraint::Equatable => self.equatable(ty, &mut HashSet::new()),
            Constraint::Orderable => match ty {
                Type::Int | Type::Float | Type::String | Type::Char | Type::Unknown | Type::Never => true,
                Type::Named { id, .. } => {
                    matches!(self.table.get(*id).kind, TypeKind::Param(Constraint::Orderable))
                }
                _ => false,
            },
        }
    }

    fn equatable(&self, ty: &Type, visited: &mut HashSet<TypeId>) -> bool {
        if self.is_move_only(ty) {
            return false;
        }
        match ty {
            Type::List(inner) => self.equatable(inner, visited),
            Type::Function { .. } | Type::Module(_) => false,
            Type::Named { id, args } | Type::Variant { id, args, .. } => {
                let b = self.builtins;
                match &self.table.get(*id).kind {
                    TypeKind::Param(constraint) => *constraint >= Constraint::Equatable,
                    TypeKind::Builtin { .. } => [b.datetime, b.response, b.regex_match].contains(id),
                    _ if !visited.insert(*id) => true,
                    TypeKind::Shape(fields) => fields
                        .iter()
                        .all(|(_, field)| self.equatable(&field.substitute(args), visited)),
                    TypeKind::Sum(variants) => variants.iter().all(|variant| {
                        variant
                            .fields
                            .iter()
                            .all(|(_, field)| self.equatable(&field.substitute(args), visited))
                    }),
                    TypeKind::Map { key, value } => {
                        self.equatable(&key.substitute(args), visited) && self.equatable(&value.substitute(args), visited)
                    }
                    TypeKind::Set { item } => self.equatable(&item.substitute(args), visited),
                    TypeKind::Pending => true,
                }
            }
            _ => true,
        }
    }

    /// The type arguments `args` must meet the `constraints` of their parameters (`what` names
    /// the generic function or type, for the message).
    pub(super) fn ensure_constraints(&self, args: &[Type], constraints: &[Constraint], what: &str) -> CheckResult<()> {
        for (arg, constraint) in args.iter().zip(constraints) {
            if !self.satisfies(arg, *constraint) {
                return self.err(format!(
                    "{} is not {}, as '{}' requires",
                    self.show(arg),
                    constraint.name(),
                    what
                ));
            }
        }
        Ok(())
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

    /// A call to a generic function: the type arguments are inferred from the arguments and the
    /// expected type, then checked against the constraints.
    pub(super) fn check_generic_call(
        &mut self,
        callee: GenericCallee,
        args: &[AstNode],
        expected: Option<&Type>,
    ) -> CheckResult<Type> {
        let GenericCallee {
            signature,
            mut generic,
            constraints,
            modes,
            name: what,
        } = callee;
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
            if wanted == Type::Float && actual == Type::Int {
                self.ownership.to_float.insert(node_id(arg));
            }
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
        self.ensure_constraints(&generic, constraints, what)?;
        Ok(ret.substitute(&generic))
    }
}
