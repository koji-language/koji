//! Statements, assignments and what can be modified.

use super::*;

impl Checker {
    pub(super) fn set_line(&mut self, node: &AstNode) {
        if let AstNode::Located { line, .. } = node {
            self.state.line = *line;
        }
    }

    /// Checks a sequence of statements in the current scope; returns the type of its value (the
    /// last statement, when it is an expression).
    pub(super) fn check_statements(&mut self, statements: &[AstNode], expected: Option<&Type>) -> CheckResult<Type> {
        self.hoist(statements)?;
        let mut tail = Type::Void;
        for (i, statement) in statements.iter().enumerate() {
            let is_last = i + 1 == statements.len();
            tail = self.check_statement(statement, if is_last { expected } else { None })?;
            if !is_last && !matches!(tail, Type::Never) {
                // The value of an expression statement is discarded.
                if !is_statement(statement) {
                    self.note_temporary(statement, &tail);
                }
                tail = Type::Void;
            }
        }
        if let Some(last) = statements.last() {
            if is_statement(last) {
                return Ok(Type::Void);
            }
        }
        Ok(tail)
    }

    pub(super) fn check_block(
        &mut self,
        node: &AstNode,
        expected: Option<&Type>,
        bindings: Vec<(String, Symbol)>,
    ) -> CheckResult<Type> {
        self.push_scope();
        for (name, symbol) in bindings {
            self.current_scope().vars.insert(name, symbol);
        }
        let result = match node.unlocated() {
            AstNode::Block(statements) => self.check_statements(statements, expected),
            other => self.check_statement(other, expected),
        };
        self.pop_scope();
        result
    }

    pub(super) fn check_statement(&mut self, node: &AstNode, expected: Option<&Type>) -> CheckResult<Type> {
        match node {
            AstNode::Located { line, node } => {
                self.state.line = *line;
                self.check_statement(node, expected)
            }
            AstNode::Export(node) => self.check_statement(node, expected),
            AstNode::VarDecl {
                name,
                ty,
                value,
                is_const,
                is_imut,
            } => {
                let declared = match ty {
                    Some(ty) => Some(self.resolve_type(ty)?),
                    None => None,
                };
                let value_ty = self.check_value(value, declared.as_ref())?;
                let var_ty = match declared {
                    Some(declared) => {
                        if !self.assignable(&declared, &value_ty) {
                            return self.err(format!(
                                "cannot assign a value of type {} to '{}' of type {}",
                                self.show(&value_ty),
                                name,
                                self.show(&declared)
                            ));
                        }
                        declared
                    }
                    None => value_ty,
                };
                self.consume(value, &var_ty)?;
                if self.is_move_only(&var_ty) {
                    self.ownership.declares.insert(node_id(node));
                }
                let mut symbol = Symbol::value(var_ty, *is_const);
                symbol.is_imut = *is_imut;
                self.declare(name, symbol)?;
                Ok(Type::Void)
            }
            AstNode::Assign { target, op, value } => {
                self.check_assign(node, target, op, value)?;
                Ok(Type::Void)
            }
            AstNode::FunctionDecl {
                name,
                owner: Some(owner),
                params,
                body,
                ..
            } => {
                let full = format!("{}.{}", owner, name);
                let info = match self.lookup_type(owner) {
                    Some(TypeRef::Type(Type::Named { id, .. }) | TypeRef::Generic(id)) => {
                        self.methods.get(&(*id, name.clone())).cloned()
                    }
                    _ => None,
                };
                let info = match info {
                    Some(info) => info,
                    None => return self.err(format!("unknown method '{}'", full)),
                };
                // Inside the body, the type parameters are opaque types.
                let opaque = self.opaque_params(&info.type_params)?;
                let signature = info.ty.substitute(&opaque);
                self.with_type_params(&info.type_params, &opaque, |c| {
                    c.check_function(&full, params, signature, body)
                })?;
                Ok(Type::Void)
            }
            AstNode::FunctionDecl {
                name,
                type_params,
                params,
                return_type,
                body,
                ..
            } => {
                let signature = match self.state.scopes.last().and_then(|s| s.vars.get(name)) {
                    Some(symbol) => symbol.ty.clone(),
                    None => self.with_type_params(type_params, &Self::signature_params(type_params.len()), |c| {
                        c.function_signature(name, params, return_type.as_ref(), true)
                    })?,
                };
                let opaque = self.opaque_params(type_params)?;
                let signature = signature.substitute(&opaque);
                self.with_type_params(type_params, &opaque, |c| c.check_function(name, params, signature, body))?;
                Ok(Type::Void)
            }
            AstNode::TypeDecl { .. } => Ok(Type::Void),
            // Imports are processed when the block is hoisted.
            AstNode::Import { .. } => Ok(Type::Void),
            AstNode::While { condition, body } => {
                self.check_condition(condition, "while")?;
                self.state.loop_scopes.push(self.state.scopes.len());
                let result = self.check_block(body, None, vec![]);
                self.state.loop_scopes.pop();
                result?;
                Ok(Type::Void)
            }
            AstNode::For {
                variable,
                is_mut,
                iterable,
                body,
            } => {
                let iterable_ty = self.check_value(iterable, None)?;
                let item = match &iterable_ty {
                    Type::List(inner) => (**inner).clone(),
                    Type::String => Type::Char,
                    Type::Unknown => Type::Unknown,
                    // A map gives its keys; a set, its items.
                    Type::Named { .. } => match (self.map_parts(&iterable_ty), self.set_item(&iterable_ty)) {
                        (Some((key, _)), _) => key,
                        (None, Some(item)) => item,
                        _ => return self.not_iterable(&iterable_ty),
                    },
                    _ => return self.not_iterable(&iterable_ty),
                };
                self.note_temporary(iterable, &iterable_ty);
                if *is_mut && self.is_move_only(&item) {
                    return self.err(format!(
                        "'for mut' would copy each item, and {} is move-only: use 'for {} in'",
                        self.show(&item),
                        variable
                    ));
                }
                let mut symbol = Symbol::value(item, !is_mut);
                symbol.is_imut = !is_mut;
                symbol.reason = Some("the 'for' variable is immut (use 'for mut' to change it)");
                symbol.borrowed = Some("the 'for' variable");
                self.state.loop_scopes.push(self.state.scopes.len());
                let result = self.check_block(body, None, vec![(variable.clone(), symbol)]);
                self.state.loop_scopes.pop();
                result?;
                Ok(Type::Void)
            }
            AstNode::Break | AstNode::Continue => Ok(Type::Never),
            AstNode::Return(value) => {
                self.check_return(value.as_deref())?;
                Ok(Type::Never)
            }
            other => self.check_expr(other, expected),
        }
    }

    pub(super) fn not_iterable<T>(&self, ty: &Type) -> CheckResult<T> {
        self.err(format!(
            "cannot iterate over a value of type {} (use a list, a string or range())",
            self.show(ty)
        ))
    }

    pub(super) fn check_condition(&mut self, condition: &AstNode, keyword: &str) -> CheckResult<()> {
        let ty = self.check_value(condition, Some(&Type::Boolean))?;
        if !matches!(ty, Type::Boolean | Type::Unknown | Type::Never) {
            return self.err(format!(
                "the condition of '{}' must be boolean, found {}",
                keyword,
                self.show(&ty)
            ));
        }
        Ok(())
    }

    pub(super) fn check_return(&mut self, value: Option<&AstNode>) -> CheckResult<()> {
        let declared = match self.state.returns.last() {
            Some(context) => context.declared.clone(),
            None => return self.err("'return' outside of a function".to_string()),
        };
        let value_ty = match value {
            Some(value) => {
                let ty = self.check_value(value, declared.as_ref())?;
                self.consume(value, &ty)?;
                Some(ty)
            }
            None => None,
        };
        match (declared, value_ty) {
            (Some(Type::Void), Some(_)) => {
                self.err("cannot return a value from a function without a return type".to_string())
            }
            (Some(Type::Void), None) => Ok(()),
            (Some(declared), None) => self.err(format!(
                "this function must return a value of type {}",
                self.show(&declared)
            )),
            (Some(declared), Some(actual)) => {
                if self.assignable(&declared, &actual) {
                    Ok(())
                } else {
                    self.err(format!(
                        "returning {}, but the function returns {}",
                        self.show(&actual),
                        self.show(&declared)
                    ))
                }
            }
            (None, actual) => {
                let actual = actual.unwrap_or(Type::Void);
                let previous = self.state.returns.last().and_then(|c| c.inferred.clone());
                let inferred = match previous {
                    None => actual,
                    Some(previous) => match self.join(&previous, &actual) {
                        Some(joined) => joined,
                        None => {
                            return self.err(format!(
                                "inconsistent return types: {} and {}",
                                self.show(&previous),
                                self.show(&actual)
                            ))
                        }
                    },
                };
                if let Some(context) = self.state.returns.last_mut() {
                    context.inferred = Some(inferred);
                }
                Ok(())
            }
        }
    }

    pub(super) fn check_assign(
        &mut self,
        node: &AstNode,
        target: &AstNode,
        op: &AssignOperator,
        value: &AstNode,
    ) -> CheckResult<()> {
        match target {
            AstNode::Identifier(name) => {
                let existing = self.lookup(name).cloned();
                let symbol = match existing {
                    Some(symbol) => symbol,
                    // Declarations always have a keyword: `x = 1` only changes an existing `x`.
                    None => {
                        return self.err(format!(
                            "'{}' is not declared: declare it with 'mut {} = ...' (or 'immut' if it never changes)",
                            name, name
                        ))
                    }
                };
                if let Some(owner) = &symbol.owner {
                    return self.err(format!("'{}' is read-only outside '{}'", name, owner));
                }
                self.ensure_not_captured(name)?;
                if !matches!(symbol.kind, SymbolKind::Value) || matches!(symbol.ty, Type::Module(_)) {
                    return self.err(format!("cannot assign to '{}'", name));
                }
                if symbol.is_imut || symbol.is_const {
                    return self.err(format!("cannot modify '{}': {}", name, self.imut_reason(&symbol)));
                }
                let new_ty = self.check_assigned_value(&symbol.ty, op, value, name)?;
                self.consume(value, &new_ty)?;
                if self.is_move_only(&symbol.ty) {
                    // The old value is dropped, and a moved variable gets a value again.
                    self.ownership.replaces.insert(node_id(node));
                    if let Some(index) = self.scope_of(name) {
                        self.state.moved.remove(&(index, name.clone()));
                    }
                }
                if symbol.ty.contains_unknown() {
                    let refined = self.join(&symbol.ty, &new_ty).unwrap_or(symbol.ty.clone());
                    if let Some(symbol) = self.lookup_mut(name) {
                        symbol.ty = refined;
                    }
                }
                Ok(())
            }
            AstNode::Index { target: inner, index } => {
                self.ensure_contents_mutable(inner)?;
                let container = self.check_value(inner, None)?;
                let index_ty = self.check_value(index, None)?;
                let element = match &container {
                    Type::List(element) => {
                        self.expect_type(&Type::Int, &index_ty, "list index")?;
                        (**element).clone()
                    }
                    Type::Unknown => Type::Unknown,
                    Type::Named { .. } => match self.map_parts(&container) {
                        Some((key, value)) => {
                            self.expect_type(&key, &index_ty, "map key")?;
                            value
                        }
                        None => return self.err(format!("cannot assign by index to {}", self.show(&container))),
                    },
                    Type::String => return self.err("strings cannot be modified".to_string()),
                    other => return self.err(format!("cannot assign by index to {}", self.show(other))),
                };
                let value_ty = self.check_assigned_value(&element, op, value, "element")?;
                self.consume(value, &value_ty)?;
                if self.is_move_only(&element) {
                    self.ownership.replaces.insert(node_id(node));
                }
                Ok(())
            }
            AstNode::Member { object, name } => {
                self.ensure_contents_mutable(object)?;
                let object_ty = self.check_value(object, None)?;
                let field_ty = match &object_ty {
                    Type::Unknown => Type::Unknown,
                    Type::Module(id) => {
                        return self.err(format!(
                            "'{}' is read-only outside '{}'",
                            name, self.modules[*id].display
                        ))
                    }
                    Type::Variant { .. } => {
                        return self.err(format!("variant fields cannot be modified ('{}')", name))
                    }
                    Type::Named { id, .. }
                        if matches!(self.table.get(*id).kind, TypeKind::Builtin { .. }) =>
                    {
                        return self.err(format!("'{}' is read-only", name))
                    }
                    other => match self.table.fields_of(other) {
                        Some(fields) => match fields.into_iter().find(|(n, _)| n == name) {
                            Some((_, ty)) => ty,
                            None => {
                                return self.err(format!(
                                    "{} has no field '{}'",
                                    self.show(other),
                                    name
                                ))
                            }
                        },
                        None => {
                            return self.err(format!(
                                "cannot assign field '{}' on {}",
                                name,
                                self.show(other)
                            ))
                        }
                    },
                };
                let value_ty = self.check_assigned_value(&field_ty, op, value, name)?;
                self.consume(value, &value_ty)?;
                if self.is_move_only(&field_ty) {
                    self.ownership.replaces.insert(node_id(node));
                }
                Ok(())
            }
            _ => self.err("invalid assignment target".to_string()),
        }
    }

    /// Changing a field or an element (`person.age = 31`, `items[0] = 1`) is not allowed when the
    /// variable that holds the value is `immut`.
    pub(super) fn ensure_contents_mutable(&self, target: &AstNode) -> CheckResult<()> {
        let mut node = target.unlocated();
        loop {
            match node {
                // `module.items[0] = 1`: a variable of another module is read-only here.
                AstNode::Member { object, name } => {
                    if let AstNode::Identifier(module) = object.unlocated() {
                        if let Some(Type::Module(id)) = self.lookup(module).map(|s| &s.ty) {
                            return self.err(format!(
                                "'{}' is read-only outside '{}'",
                                name, self.modules[*id].display
                            ));
                        }
                    }
                    node = object.unlocated()
                }
                AstNode::Index { target, .. } => node = target.unlocated(),
                AstNode::Identifier(name) => {
                    return match self.lookup(name) {
                        Some(Symbol { owner: Some(owner), .. }) => {
                            self.err(format!("'{}' is read-only outside '{}'", name, owner))
                        }
                        Some(symbol) if symbol.is_imut && matches!(symbol.kind, SymbolKind::Value) => {
                            self.err(format!("cannot modify '{}': {}", name, self.imut_reason(symbol)))
                        }
                        _ => {
                            self.ensure_not_moved(name)?;
                            self.ensure_not_captured(name)
                        }
                    };
                }
                _ => return Ok(()),
            }
        }
    }

    pub(super) fn imut_reason(&self, symbol: &Symbol) -> &'static str {
        symbol.reason.unwrap_or("it is immut")
    }

    /// What can be passed as `mut`, or a `mut self` method called on (`receiver` is the
    /// method): a `mut` variable (not `immut`), or a field or an element of one. Returns its type.
    pub(super) fn check_mut_place(&mut self, place: &AstNode, receiver: Option<&str>) -> CheckResult<Type> {
        let what = |name: &str| match receiver {
            Some(method) => format!("cannot call '{}' on '{}', as it changes the value", method, name),
            None => format!("cannot pass '{}' as 'mut'", name),
        };
        match place.unlocated() {
            AstNode::Identifier(name) => {
                let symbol = match self.lookup(name) {
                    Some(symbol) => symbol.clone(),
                    None => return self.err(format!("undefined variable '{}'", name)),
                };
                if let Some(owner) = &symbol.owner {
                    return self.err(format!("'{}' is read-only outside '{}'", name, owner));
                }
                self.ensure_not_captured(name)?;
                self.ensure_not_moved(name)?;
                if !matches!(symbol.kind, SymbolKind::Value) || matches!(symbol.ty, Type::Module(_)) {
                    return self.err(what(name));
                }
                if symbol.is_imut || symbol.is_const {
                    return self.err(format!("{}: {}", what(name), self.imut_reason(&symbol)));
                }
                Ok(symbol.ty)
            }
            AstNode::Member { object, name } => {
                self.ensure_contents_mutable(object)?;
                let object_ty = self.check_value(object, None)?;
                let read_only = match &object_ty {
                    Type::Module(_) | Type::Variant { .. } => true,
                    Type::Named { id, .. } => matches!(self.table.get(*id).kind, TypeKind::Builtin { .. }),
                    _ => false,
                };
                if read_only {
                    return self.err(format!("{}: it is read-only", what(name)));
                }
                self.check_member(object, name)
            }
            AstNode::Index { target, .. } => {
                self.ensure_contents_mutable(target)?;
                if self.check_value(target, None)? == Type::String {
                    return self.err("strings cannot be modified".to_string());
                }
                self.check_value(place, None)
            }
            _ => match receiver {
                Some(method) => self.err(format!(
                    "'{}' changes the value it is called on: call it on a variable, a field or an element",
                    method
                )),
                None => self.err("only a variable, a field or an element can be passed as 'mut'".to_string()),
            },
        }
    }

    /// Checks `target op= value` where the target has type `target_ty`; returns the value type.
    pub(super) fn check_assigned_value(
        &mut self,
        target_ty: &Type,
        op: &AssignOperator,
        value: &AstNode,
        name: &str,
    ) -> CheckResult<Type> {
        let value_ty = match op.binary() {
            None => self.check_value(value, Some(target_ty))?,
            Some(binary) => {
                let right = self.check_value(value, None)?;
                self.binary_type(binary, target_ty, &right)?
            }
        };
        if !self.assignable(target_ty, &value_ty) {
            return self.err(format!(
                "cannot assign a value of type {} to '{}' of type {}",
                self.show(&value_ty),
                name,
                self.show(target_ty)
            ));
        }
        Ok(value_ty)
    }
}
