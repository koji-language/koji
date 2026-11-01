//! Blocks and statements: hoisting, scopes, loops and assignments.

use super::*;

impl Interpreter {
    /// Defines functions and constructors of a block before running it.
    pub(super) fn hoist(&mut self, statements: &[AstNode], env: &Rc<Env>) -> Exec<()> {
        // Imports first, as in the type checker: the whole block can use them.
        for statement in statements {
            if let AstNode::Located { line, .. } = statement {
                self.line = *line;
            }
            if let AstNode::Import { kind, path } = statement.unlocated() {
                self.import(kind, path, env)?;
            }
        }
        for statement in statements {
            match statement.unlocated() {
                AstNode::FunctionDecl {
                    name,
                    owner,
                    params,
                    return_type,
                    body,
                } => {
                    let full = match owner {
                        Some(owner) => format!("{}.{}", owner, name),
                        None => name.clone(),
                    };
                    let closure = Rc::new(Closure {
                        name: full.clone(),
                        params: params.clone(),
                        body: Rc::clone(body),
                        env: Rc::clone(env),
                        float_params: params.iter().map(|p| is_float_type(&p.ty)).collect(),
                        returns_float: is_float_type(return_type),
                        file: Rc::clone(&self.file),
                    });
                    match owner {
                        // Methods are not variables: calls find them by module and type.
                        Some(_) => {
                            self.methods.insert((self.module_path.clone(), full), closure);
                        }
                        None => env.define(name, Value::Function(closure)),
                    }
                }
                AstNode::TypeDecl {
                    name,
                    definition: TypeDef::Sum(variants),
                } => {
                    let sum = Rc::new(SumType {
                        name: name.clone(),
                        variants: variants
                            .iter()
                            .map(|v| {
                                (v.name.clone(), v.fields.iter().map(|f| f.name.clone()).collect())
                            })
                            .collect(),
                    });
                    for variant in variants {
                        env.define(&variant.name, sum.constructor(&variant.name));
                    }
                    if let Some(module) = &self.current_module {
                        if Rc::ptr_eq(&module.env, env) {
                            module.types.borrow_mut().push(sum);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn exec_statements(&mut self, statements: &[AstNode], env: &Rc<Env>) -> Exec<Value> {
        self.hoist(statements, env)?;
        let mut last = Value::Void;
        for statement in statements {
            last = self.eval(statement, env)?;
        }
        match statements.last().map(AstNode::unlocated) {
            Some(
                AstNode::While { .. }
                | AstNode::For { .. }
                | AstNode::Assign { .. }
                | AstNode::VarDecl { .. }
                | AstNode::FunctionDecl { .. }
                | AstNode::TypeDecl { .. }
                | AstNode::Import { .. },
            ) => Ok(Value::Void),
            _ => Ok(last),
        }
    }

    /// Runs a block (or a single expression, e.g. a match arm) in a new scope.
    pub(super) fn exec_block(&mut self, node: &AstNode, env: &Rc<Env>) -> Exec<Value> {
        let scope = Env::child(env);
        match node.unlocated() {
            AstNode::Block(statements) => self.exec_statements(statements, &scope),
            other => self.eval(other, &scope),
        }
    }

    pub(super) fn exec_loop_body(&mut self, body: &AstNode, env: &Rc<Env>) -> Exec<bool> {
        match self.exec_block(body, env) {
            Ok(_) | Err(Flow::Continue) => Ok(true),
            Err(Flow::Break) => Ok(false),
            Err(other) => Err(other),
        }
    }

    pub(super) fn eval_assign(
        &mut self,
        target: &AstNode,
        op: &AssignOperator,
        value: &AstNode,
        env: &Rc<Env>,
    ) -> Exec<()> {
        // `x = value` on a new name declares a variable.
        if let AstNode::Identifier(name) = target.unlocated() {
            if env.get(name).is_none() {
                let value = self.eval(value, env)?;
                env.define(name, value);
                return Ok(());
            }
        }
        // The place first (with its indices), then the value, as written from left to right.
        let place = self.resolve_place(target, env)?;
        let current = self.read_place(&place, env)?;
        let value = self.eval(value, env)?;
        let value = match (op.binary(), current) {
            (None, Some(Value::Float(_))) => coerce(value, true),
            (None, _) => value,
            (Some(binary), Some(current)) => {
                let is_float = matches!(current, Value::Float(_));
                coerce(self.binary(binary, current, value)?, is_float)
            }
            (Some(_), None) => {
                return Err(self.error(match place.steps.last().and_then(Step::key) {
                    Some(key) => format!("key '{}' not found", key),
                    None => "compound assignment to an undefined value".to_string(),
                }))
            }
        };
        self.write_place(&place, value, env)
    }
}
