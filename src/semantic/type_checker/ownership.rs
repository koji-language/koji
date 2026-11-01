//! Move-only values: which types are move-only, moves, and the use of a moved variable.
//!
//! A type is move-only when it declares a `drop` method, or holds a move-only value (a `File`,
//! a field of a move-only type, a list of them...). A move-only value has exactly one owner:
//! assigning it, returning it or storing it in another value moves it, and the variable it came
//! from cannot be used anymore. Parameters, `self`, the `for` variable and the bindings of a
//! `match` on a variable only borrow their value: they can read it, but not move it.

use super::*;

impl Checker {
    pub(super) fn is_move_only(&self, ty: &Type) -> bool {
        self.move_only_in(ty, &mut HashSet::new())
    }

    fn move_only_in(&self, ty: &Type, visited: &mut HashSet<TypeId>) -> bool {
        match ty {
            Type::List(inner) => self.move_only_in(inner, visited),
            Type::Named { id, args } | Type::Variant { id, args, .. } => {
                if *id == self.builtins.file || self.methods.contains_key(&(*id, "drop".to_string())) {
                    return true;
                }
                if args.iter().any(|arg| self.move_only_in(arg, visited)) {
                    return true;
                }
                if !visited.insert(*id) {
                    return false;
                }
                match &self.table.get(*id).kind {
                    TypeKind::Shape(fields) => fields
                        .iter()
                        .any(|(_, field)| self.move_only_in(&field.substitute(args), visited)),
                    TypeKind::Sum(variants) => variants.iter().any(|variant| {
                        variant
                            .fields
                            .iter()
                            .any(|(_, field)| self.move_only_in(&field.substitute(args), visited))
                    }),
                    TypeKind::Dict { value, .. } => self.move_only_in(value, visited),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Index of the scope that declares `name`.
    pub(super) fn scope_of(&self, name: &str) -> Option<usize> {
        self.state.scopes.iter().rposition(|scope| scope.vars.contains_key(name))
    }

    /// Reading a variable whose value was moved is an error.
    pub(super) fn ensure_not_moved(&self, name: &str) -> CheckResult<()> {
        let index = match self.scope_of(name) {
            Some(index) => index,
            None => return Ok(()),
        };
        match self.state.moved.get(&(index, name.to_string())) {
            Some(line) => self.err(format!(
                "'{}' cannot be used here: its value was moved (or closed) on line {}; assign it a new value first",
                name, line
            )),
            None => Ok(()),
        }
    }

    /// A function cannot use a move-only variable of the functions around it: it would get a
    /// copy of it.
    pub(super) fn ensure_not_captured_move_only(&self, name: &str, ty: &Type) -> CheckResult<()> {
        let boundary = match self.state.function_scopes.last() {
            Some(boundary) => *boundary,
            None => return Ok(()),
        };
        match self.scope_of(name) {
            Some(index) if index > 1 && index < boundary && self.is_move_only(ty) => self.err(format!(
                "'{}' cannot be used here: functions capture a copy of the variables around them, and '{}' cannot be copied",
                name,
                self.show(ty)
            )),
            _ => Ok(()),
        }
    }

    /// `value` (of type `ty`) is stored, returned or assigned: when it is a move-only variable,
    /// the variable gives up its value.
    pub(super) fn consume(&mut self, value: &AstNode, ty: &Type) -> CheckResult<()> {
        if !self.is_move_only(ty) {
            return Ok(());
        }
        match value.unlocated() {
            AstNode::Identifier(name) => self.move_variable(value, name, ty),
            AstNode::Member { .. } | AstNode::Index { .. } => self.err(format!(
                "a value of type {} cannot be copied out of a field or an element (it is move-only)",
                self.show(ty)
            )),
            node @ (AstNode::If { .. } | AstNode::Match { .. } | AstNode::Block(_)) => {
                if Self::has_variable_tail(node) {
                    return self.err(format!(
                        "a value of type {} cannot be moved out of a variable inside an 'if', a 'match' or a block; move it first ('x = value')",
                        self.show(ty)
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Whether a branch of `node` ends with a variable (`if (c) { f } else { g }`).
    fn has_variable_tail(node: &AstNode) -> bool {
        match node.unlocated() {
            AstNode::Identifier(_) => true,
            AstNode::Block(statements) => statements.last().is_some_and(Self::has_variable_tail),
            AstNode::If {
                then_branch,
                else_branch,
                ..
            } => Self::has_variable_tail(then_branch) || else_branch.as_deref().is_some_and(Self::has_variable_tail),
            AstNode::Match { arms, .. } => arms.iter().any(|arm| Self::has_variable_tail(&arm.body)),
            _ => false,
        }
    }

    /// Moves the value out of the variable `name` (read by `node`).
    pub(super) fn move_variable(&mut self, node: &AstNode, name: &str, ty: &Type) -> CheckResult<()> {
        let symbol = match self.lookup(name) {
            Some(symbol) => symbol.clone(),
            None => return Ok(()),
        };
        if !matches!(symbol.kind, SymbolKind::Value) {
            return Ok(());
        }
        let what = self.show(ty);
        if let Some(owner) = &symbol.owner {
            return self.err(format!("'{}' cannot be moved: it belongs to '{}'", name, owner));
        }
        if let Some(borrowed) = symbol.borrowed {
            return self.err(format!(
                "'{}' cannot be moved: it is {}, which only borrows its value ({} is move-only)",
                name, borrowed, what
            ));
        }
        let index = self.scope_of(name).unwrap_or(0);
        if let Some(boundary) = self.state.function_scopes.last() {
            if index < *boundary {
                return self.err(format!(
                    "'{}' cannot be moved here: it is declared outside this function, which may run more than once",
                    name
                ));
            }
        }
        if let Some(start) = self.state.loop_scopes.last() {
            if index < *start {
                return self.err(format!(
                    "'{}' cannot be moved inside a loop: it is declared outside the loop, and the next iteration would use it again",
                    name
                ));
            }
        }
        let line = self.state.line;
        self.state.moved.insert((index, name.to_string()), line);
        // The variable narrowed by `if (x is ...)` is the same as the one outside.
        if symbol.narrowed {
            if let Some(outer) = self.state.scopes[..index]
                .iter()
                .rposition(|scope| scope.vars.contains_key(name))
            {
                self.state.moved.insert((outer, name.to_string()), line);
            }
        }
        self.ownership.moves.insert(node_id(node));
        Ok(())
    }

    /// A move-only value used in place by a temporary expression (`file.open(...)?` as a
    /// statement or an argument) is dropped after that use.
    pub(super) fn note_temporary(&mut self, node: &AstNode, ty: &Type) {
        if self.is_move_only(ty) && Self::is_temporary(node) {
            self.ownership.temporaries.insert(node_id(node));
        }
    }

    pub(super) fn is_temporary(node: &AstNode) -> bool {
        match node.unlocated() {
            AstNode::Call { .. } | AstNode::ObjectLiteral(_) | AstNode::ArrayLiteral(_) => true,
            AstNode::Try { value, .. } => Self::is_temporary(value),
            _ => false,
        }
    }

    pub(super) fn is_place(node: &AstNode) -> bool {
        matches!(
            node.unlocated(),
            AstNode::Identifier(_) | AstNode::Member { .. } | AstNode::Index { .. }
        )
    }

    /// The moves of the branches of an `if` or a `match`: a variable moved in any branch that
    /// continues is moved after it. `branches` holds the moves after each branch and whether it
    /// ends the function or the loop (`return`, `break`...).
    pub(super) fn merge_moves(&mut self, before: Moves, branches: Vec<(Moves, bool)>) {
        let len = self.state.scopes.len();
        let mut merged = before;
        for (moved, diverges) in branches {
            if diverges {
                continue;
            }
            for (key, line) in moved {
                if key.0 < len {
                    merged.entry(key).or_insert(line);
                }
            }
        }
        self.state.moved = merged;
    }
}
