//! `Map[K: equatable, V]` and `Set[T: equatable]`: their literals and methods.

use super::*;

impl Checker {
    /// The key and value types of a `Map[K, V]`.
    pub(super) fn map_parts(&self, ty: &Type) -> Option<(Type, Type)> {
        match ty {
            Type::Named { id, args } => match &self.table.get(*id).kind {
                TypeKind::Map { key, value } => Some((key.substitute(args), value.substitute(args))),
                _ => None,
            },
            _ => None,
        }
    }

    /// The item type of a `Set[T]`.
    pub(super) fn set_item(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::Named { id, args } => match &self.table.get(*id).kind {
                TypeKind::Set { item } => Some(item.substitute(args)),
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn map_of(&self, key: Type, value: Type) -> Type {
        Type::Named {
            id: self.builtins.map,
            args: vec![key, value],
        }
    }

    /// `['Ana': 30, 'Bia': 25]`, or `[:]`.
    pub(super) fn check_map_literal(&mut self, entries: &[(AstNode, AstNode)], expected: Option<&Type>) -> CheckResult<Type> {
        let (mut key_ty, mut value_ty) = match expected.and_then(|ty| self.map_parts(ty)) {
            Some(parts) => parts,
            None => (Type::Unknown, Type::Unknown),
        };
        for (key, value) in entries {
            let wanted_key = (!key_ty.contains_unknown()).then(|| key_ty.clone());
            let actual_key = self.check_value(key, wanted_key.as_ref())?;
            key_ty = self.join_entry(&key_ty, &actual_key, "keys")?;
            let wanted_value = (!value_ty.contains_unknown()).then(|| value_ty.clone());
            let actual_value = self.check_value(value, wanted_value.as_ref())?;
            self.consume(value, &actual_value)?;
            value_ty = self.join_entry(&value_ty, &actual_value, "values")?;
        }
        if !self.satisfies(&key_ty, Constraint::Equatable) {
            return self.err(format!("map keys must be equatable, and {} is not", self.show(&key_ty)));
        }
        Ok(self.map_of(key_ty, value_ty))
    }

    fn join_entry(&self, so_far: &Type, actual: &Type, what: &str) -> CheckResult<Type> {
        if !so_far.contains_unknown() && self.assignable(so_far, actual) {
            return Ok(so_far.clone());
        }
        match self.join(so_far, actual) {
            Some(joined) => Ok(joined),
            None => self.err(format!(
                "map {} must have the same type: found {} and {}",
                what,
                self.show(so_far),
                self.show(actual)
            )),
        }
    }

    /// The methods of maps and sets. `remove` (both) and `add` (sets) change the value, like
    /// `mut self` methods.
    pub(super) fn check_collection_method(
        &mut self,
        callee: &AstNode,
        object: &AstNode,
        object_ty: &Type,
        name: &str,
        args: &[AstNode],
    ) -> CheckResult<Option<Type>> {
        let (key, value, is_map) = match (self.map_parts(object_ty), self.set_item(object_ty)) {
            (Some((key, value)), _) => (key, value, true),
            (None, Some(item)) => (item, Type::Void, false),
            _ => return Ok(None),
        };
        let mutating = name == "remove" || (!is_map && name == "add");
        let signature: (Vec<Type>, Type) = match (name, is_map) {
            ("get", true) => (vec![key.clone()], self.option_of(value.clone())),
            ("has", _) => (vec![key.clone()], Type::Boolean),
            ("remove", true) => (vec![key.clone()], self.option_of(value.clone())),
            ("keys", true) => (vec![], Type::List(Box::new(key.clone()))),
            ("values", true) => (vec![], Type::List(Box::new(value.clone()))),
            ("values", false) => (vec![], Type::List(Box::new(key.clone()))),
            ("add", false) => (vec![key.clone()], Type::Void),
            ("remove", false) => (vec![key.clone()], Type::Boolean),
            ("length", _) => (vec![], Type::Int),
            _ => return Ok(None),
        };
        if mutating {
            self.check_mut_place(object, Some(name))?;
            self.ownership.mutating_calls.insert(node_id(callee));
        }
        let arg_types = self.check_args(&signature.0, args)?;
        // `seen = Set.from([])` then `seen.add(1)`: the set becomes a Set[int].
        if !is_map && name == "add" && key.contains_unknown() {
            let set = Type::Named {
                id: self.builtins.set,
                args: arg_types,
            };
            self.refine_variable(object, set);
        }
        Ok(Some(signature.1))
    }

    /// `Set.from([1, 2])`
    pub(super) fn check_set_from(&mut self, args: &[AstNode], expected: Option<&Type>) -> CheckResult<Type> {
        let wanted = expected
            .and_then(|ty| self.set_item(ty))
            .map(|item| Type::List(Box::new(item)));
        if args.len() != 1 {
            return self.err(format!("'Set.from' expects 1 argument(s), found {}", args.len()));
        }
        let items = self.check_value(&args[0], wanted.as_ref())?;
        let item = match items {
            Type::List(item) => *item,
            Type::Unknown => Type::Unknown,
            other => return self.err(format!("'Set.from' needs a list, found {}", self.show(&other))),
        };
        if !self.satisfies(&item, Constraint::Equatable) {
            return self.err(format!("set items must be equatable, and {} is not", self.show(&item)));
        }
        Ok(Type::Named {
            id: self.builtins.set,
            args: vec![item],
        })
    }
}
