//! Lowering: from the checked AST to the IR.
//!
//! The types come from the type checker (`Ownership::types`); this pass only chooses the typed
//! operations, makes conversions explicit (an `int` passed where a `float` is expected) and
//! turns control flow into basic blocks. A construct the compiler does not support yet is a
//! `CompileError` that names it.

use std::collections::HashMap;
use std::path::Path;

use super::*;
use crate::error::EzError;
use crate::parser::ast::{
    AssignOperator, AstNode, BinaryOperator, TemplatePart, TypeExpr, UnaryOperator,
};
use crate::semantic::type_checker::{node_id, Program as CheckedProgram};
use crate::semantic::types::Type;

type Lower<T> = Result<T, EzError>;

/// Lowers the main file of a checked program.
pub fn lower_program(program: &CheckedProgram) -> Lower<Program> {
    let statements = &program.modules[&program.entry];
    let file = display_path(&program.entry);
    let mut lowerer = Lowerer {
        types: &program.ownership.types,
        file,
        line: 0,
        functions: Vec::new(),
        signatures: HashMap::new(),
        globals: Vec::new(),
        global_names: HashMap::new(),
        strings: Vec::new(),
        string_ids: HashMap::new(),
    };
    lowerer.declare_functions(statements)?;
    lowerer.declare_globals(statements)?;
    let main = lowerer.lower_main(statements)?;
    lowerer.lower_functions(statements)?;
    let functions = lowerer
        .functions
        .into_iter()
        .map(|function| function.expect("every declared function is lowered"))
        .collect();
    Ok(Program {
        file: lowerer.file,
        functions,
        main,
        globals: lowerer.globals,
        strings: lowerer.strings,
    })
}

/// The file name used in messages: relative to the current directory when possible, like the
/// interpreter.
fn display_path(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| path.display().to_string())
}

struct Signature {
    id: FuncId,
    params: Vec<Ty>,
    ret: Ty,
}

struct Lowerer<'a> {
    types: &'a HashMap<usize, Type>,
    file: String,
    line: usize,
    functions: Vec<Option<Function>>,
    signatures: HashMap<String, Signature>,
    globals: Vec<Global>,
    global_names: HashMap<String, GlobalId>,
    strings: Vec<String>,
    string_ids: HashMap<String, StrId>,
}

/// A name in scope.
#[derive(Clone, Copy)]
enum Variable {
    Local(LocalId),
    Global(GlobalId),
}

/// The function being lowered.
struct Builder {
    locals: Vec<Local>,
    blocks: Vec<Block>,
    current: BlockId,
    /// Whether the current block already has its terminator (code after `return` is dead).
    terminated: bool,
    scopes: Vec<HashMap<String, Variable>>,
    /// `(continue target, break target)` of the enclosing loops.
    loops: Vec<(BlockId, BlockId)>,
    ret: Ty,
    /// Lowering the top-level statements: their declarations are globals.
    is_main: bool,
}

impl Builder {
    fn new(ret: Ty, is_main: bool) -> Builder {
        Builder {
            locals: Vec::new(),
            blocks: vec![Block {
                instructions: Vec::new(),
                terminator: Terminator::Unreachable,
            }],
            current: 0,
            terminated: false,
            scopes: vec![HashMap::new()],
            loops: Vec::new(),
            ret,
            is_main,
        }
    }

    fn local(&mut self, ty: Ty, name: Option<&str>) -> LocalId {
        self.locals.push(Local {
            ty,
            name: name.map(str::to_string),
        });
        self.locals.len() - 1
    }

    fn new_block(&mut self) -> BlockId {
        self.blocks.push(Block {
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        self.blocks.len() - 1
    }

    fn emit(&mut self, instruction: Instruction) {
        if !self.terminated {
            self.blocks[self.current].instructions.push(instruction);
        }
    }

    fn terminate(&mut self, terminator: Terminator) {
        if !self.terminated {
            self.blocks[self.current].terminator = terminator;
            self.terminated = true;
        }
    }

    /// Continues in `block`.
    fn switch_to(&mut self, block: BlockId) {
        self.current = block;
        self.terminated = false;
    }

    fn lookup(&self, name: &str) -> Option<Variable> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name).copied())
    }

    fn finish(self, name: String, params: usize) -> Function {
        Function {
            name,
            params,
            ret: self.ret,
            locals: self.locals,
            blocks: self.blocks,
        }
    }
}

impl Lowerer<'_> {
    fn unsupported<T>(&self, what: &str) -> Lower<T> {
        let location = if self.line == 0 {
            self.file.clone()
        } else {
            format!("{}:{}", self.file, self.line)
        };
        Err(EzError::CompileError(format!(
            "{}: {} is not supported by 'kojic build' yet (use 'kojic run')",
            location, what
        )))
    }

    fn ty(&self, ty: &Type) -> Lower<Ty> {
        match ty {
            Type::Int => Ok(Ty::Int),
            Type::Float => Ok(Ty::Float),
            Type::Boolean => Ok(Ty::Bool),
            Type::String => Ok(Ty::Str),
            Type::Void | Type::Never => Ok(Ty::Void),
            Type::Char => self.unsupported("the type 'char'"),
            Type::List(_) => self.unsupported("a list"),
            Type::Function { .. } => self.unsupported("a function value"),
            Type::Named { .. } | Type::Variant { .. } => {
                self.unsupported("a value of a declared or built-in type (Option, Result, Map, ...)")
            }
            _ => self.unsupported("this type"),
        }
    }

    fn type_expr(&self, ty: &TypeExpr) -> Lower<Ty> {
        match ty {
            TypeExpr::Named(name) => match name.as_str() {
                "int" => Ok(Ty::Int),
                "float" => Ok(Ty::Float),
                "boolean" => Ok(Ty::Bool),
                "string" => Ok(Ty::Str),
                "void" => Ok(Ty::Void),
                _ => self.unsupported(&format!("the type '{}'", name)),
            },
            TypeExpr::Array(_) => self.unsupported("a list"),
            _ => self.unsupported("this type"),
        }
    }

    /// The type of an expression, as found by the checker.
    fn type_of(&self, node: &AstNode) -> Lower<Ty> {
        match self.types.get(&node_id(node)) {
            Some(ty) => self.ty(ty),
            None => self.unsupported("this expression"),
        }
    }

    fn string(&mut self, text: &str) -> Operand {
        if let Some(id) = self.string_ids.get(text) {
            return Operand::Str(*id);
        }
        self.strings.push(text.to_string());
        let id = self.strings.len() - 1;
        self.string_ids.insert(text.to_string(), id);
        Operand::Str(id)
    }

    // Declarations

    /// Gives every top-level function an id first: functions can be called before they are
    /// declared.
    fn declare_functions(&mut self, statements: &[AstNode]) -> Lower<()> {
        // The top-level statements are function 0.
        self.functions.push(None);
        for statement in statements {
            if let AstNode::Located { line, .. } = statement {
                self.line = *line;
            }
            if let AstNode::FunctionDecl {
                name,
                owner,
                type_params,
                params,
                return_type,
                ..
            } = statement.unlocated()
            {
                if owner.is_some() {
                    return self.unsupported("a method");
                }
                if !type_params.is_empty() {
                    return self.unsupported("a generic function");
                }
                let mut param_types = Vec::new();
                for param in params {
                    if param.is_mut || param.is_take {
                        return self.unsupported("a 'mut' or 'take' parameter");
                    }
                    match &param.ty {
                        Some(ty) => param_types.push(self.type_expr(ty)?),
                        None => return self.unsupported("a parameter without a type"),
                    }
                }
                let ret = match return_type {
                    Some(ty) => self.type_expr(ty)?,
                    None => Ty::Void,
                };
                self.functions.push(None);
                self.signatures.insert(
                    name.clone(),
                    Signature {
                        id: self.functions.len() - 1,
                        params: param_types,
                        ret,
                    },
                );
            }
        }
        Ok(())
    }

    /// Top-level variables are globals, so functions can use them.
    fn declare_globals(&mut self, statements: &[AstNode]) -> Lower<()> {
        for statement in statements {
            if let AstNode::Located { line, .. } = statement {
                self.line = *line;
            }
            if let AstNode::VarDecl { name, ty, value, .. } = statement.unlocated() {
                let ty = match ty {
                    Some(ty) => self.type_expr(ty)?,
                    None => self.type_of(value)?,
                };
                self.globals.push(Global {
                    ty,
                    name: name.clone(),
                });
                self.global_names.insert(name.clone(), self.globals.len() - 1);
            }
        }
        Ok(())
    }

    fn lower_main(&mut self, statements: &[AstNode]) -> Lower<FuncId> {
        let mut builder = Builder::new(Ty::Void, true);
        self.line = 0;
        self.statements(&mut builder, statements)?;
        builder.terminate(Terminator::Return(None));
        self.functions[0] = Some(builder.finish("main".to_string(), 0));
        Ok(0)
    }

    fn lower_functions(&mut self, statements: &[AstNode]) -> Lower<()> {
        for statement in statements {
            if let AstNode::Located { line, .. } = statement {
                self.line = *line;
            }
            if let AstNode::FunctionDecl {
                name, params, body, ..
            } = statement.unlocated()
            {
                let signature = &self.signatures[name];
                let (id, param_types, ret) = (signature.id, signature.params.clone(), signature.ret);
                let mut builder = Builder::new(ret, false);
                for (param, ty) in params.iter().zip(&param_types) {
                    let local = builder.local(*ty, Some(&param.name));
                    builder.scopes[0].insert(param.name.clone(), Variable::Local(local));
                }
                builder.scopes.push(HashMap::new());
                match body.unlocated() {
                    AstNode::Block(statements) => self.statements(&mut builder, statements)?,
                    // `function double(x: int): int => x * 2`
                    expression => {
                        let value = self.expr(&mut builder, expression)?;
                        let value = self.convert(&mut builder, value, self.type_of(expression)?, ret)?;
                        builder.terminate(Terminator::Return(value));
                    }
                }
                if ret == Ty::Void {
                    builder.terminate(Terminator::Return(None));
                }
                self.functions[id] = Some(builder.finish(name.clone(), params.len()));
            }
        }
        Ok(())
    }

    // Statements

    fn statements(&mut self, builder: &mut Builder, statements: &[AstNode]) -> Lower<()> {
        for statement in statements {
            self.statement(builder, statement)?;
        }
        Ok(())
    }

    fn block(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<()> {
        builder.scopes.push(HashMap::new());
        let result = match node.unlocated() {
            AstNode::Block(statements) => self.statements(builder, statements),
            other => self.statement(builder, other),
        };
        builder.scopes.pop();
        result
    }

    fn statement(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<()> {
        match node {
            AstNode::Located { line, node } => {
                self.line = *line;
                self.statement(builder, node)
            }
            AstNode::Export(node) => self.statement(builder, node),
            AstNode::FunctionDecl { .. } if builder.is_main && builder.scopes.len() == 1 => Ok(()),
            AstNode::FunctionDecl { .. } => self.unsupported("a nested function"),
            AstNode::TypeDecl { .. } => self.unsupported("a type declaration"),
            AstNode::Import { .. } => self.unsupported("an import"),
            AstNode::VarDecl { name, ty, value, .. } => {
                let value_type = self.type_of(value)?;
                let var_type = match ty {
                    Some(ty) => self.type_expr(ty)?,
                    None => value_type,
                };
                let operand = self.expr(builder, value)?;
                let operand = self.convert(builder, operand, value_type, var_type)?;
                let place = if builder.is_main && builder.scopes.len() == 1 {
                    let id = self.global_names[name];
                    builder.scopes[0].insert(name.clone(), Variable::Global(id));
                    Place::Global(id)
                } else {
                    let id = builder.local(var_type, Some(name));
                    Place::Local(id)
                };
                if let Some(value) = operand {
                    builder.emit(Instruction::Copy { dest: place, value });
                }
                // Declared after its value: `immut x = x + 1` reads the outer `x`.
                if let Place::Local(id) = place {
                    builder
                        .scopes
                        .last_mut()
                        .expect("a scope")
                        .insert(name.clone(), Variable::Local(id));
                }
                Ok(())
            }
            AstNode::Assign { target, op, value } => self.assign(builder, target, op, value),
            AstNode::While { condition, body } => {
                let head = builder.new_block();
                let body_block = builder.new_block();
                let exit = builder.new_block();
                builder.terminate(Terminator::Goto(head));
                builder.switch_to(head);
                let condition = self.condition(builder, condition)?;
                builder.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                builder.switch_to(body_block);
                builder.loops.push((head, exit));
                self.block(builder, body)?;
                builder.loops.pop();
                builder.terminate(Terminator::Goto(head));
                builder.switch_to(exit);
                Ok(())
            }
            AstNode::For {
                variable,
                iterable,
                body,
                ..
            } => self.for_range(builder, variable, iterable, body),
            AstNode::Break => {
                let Some(&(_, exit)) = builder.loops.last() else {
                    return self.unsupported("'break' outside a loop");
                };
                builder.terminate(Terminator::Goto(exit));
                Ok(())
            }
            AstNode::Continue => {
                let Some(&(head, _)) = builder.loops.last() else {
                    return self.unsupported("'continue' outside a loop");
                };
                builder.terminate(Terminator::Goto(head));
                Ok(())
            }
            AstNode::Return(value) => {
                let operand = match value {
                    Some(value) => {
                        let operand = self.expr(builder, value)?;
                        self.convert(builder, operand, self.type_of(value)?, builder.ret)?
                    }
                    None => None,
                };
                builder.terminate(Terminator::Return(operand));
                Ok(())
            }
            AstNode::Block(_) => self.block(builder, node),
            AstNode::If {
                condition,
                then_branch,
                else_branch,
            } if self.type_of(node).unwrap_or(Ty::Void) == Ty::Void => {
                self.if_statement(builder, condition, then_branch, else_branch.as_deref())
            }
            expression => {
                self.expr(builder, expression)?;
                Ok(())
            }
        }
    }

    fn if_statement(
        &mut self,
        builder: &mut Builder,
        condition: &AstNode,
        then_branch: &AstNode,
        else_branch: Option<&AstNode>,
    ) -> Lower<()> {
        let condition = self.condition(builder, condition)?;
        let then_block = builder.new_block();
        let else_block = builder.new_block();
        let exit = if else_branch.is_some() {
            builder.new_block()
        } else {
            else_block
        };
        builder.terminate(Terminator::Branch {
            condition,
            then_block,
            else_block,
        });
        builder.switch_to(then_block);
        self.block(builder, then_branch)?;
        builder.terminate(Terminator::Goto(exit));
        if let Some(else_branch) = else_branch {
            builder.switch_to(else_block);
            self.block(builder, else_branch)?;
            builder.terminate(Terminator::Goto(exit));
        }
        builder.switch_to(exit);
        Ok(())
    }

    /// `for i in range(...)`, without building the list.
    fn for_range(
        &mut self,
        builder: &mut Builder,
        variable: &str,
        iterable: &AstNode,
        body: &AstNode,
    ) -> Lower<()> {
        let args = match iterable.unlocated() {
            AstNode::Call { callee, args, .. }
                if matches!(callee.unlocated(), AstNode::Identifier(name) if name == "range"
                    && builder.lookup("range").is_none()
                    && !self.signatures.contains_key("range")) =>
            {
                args
            }
            _ => return self.unsupported("a 'for' loop over something other than 'range'"),
        };
        let mut values = Vec::new();
        for arg in args {
            let value = self.expr(builder, arg)?.expect("an int");
            // Copied: the bounds are evaluated once.
            let local = builder.local(Ty::Int, None);
            builder.emit(Instruction::Copy {
                dest: Place::Local(local),
                value,
            });
            values.push(Operand::Local(local));
        }
        let (start, end, step) = match values.as_slice() {
            [end] => (Operand::Int(0), end.clone(), Operand::Int(1)),
            [start, end] => (start.clone(), end.clone(), Operand::Int(1)),
            [start, end, step] => (start.clone(), end.clone(), step.clone()),
            _ => return self.unsupported("this call of 'range'"),
        };
        builder.emit(Instruction::RangeCheck {
            step: step.clone(),
            line: self.line,
        });
        let next = builder.local(Ty::Int, None);
        let done = builder.local(Ty::Bool, None);
        let has_value = builder.local(Ty::Bool, None);
        builder.emit(Instruction::Copy {
            dest: Place::Local(next),
            value: start,
        });
        builder.emit(Instruction::Copy {
            dest: Place::Local(done),
            value: Operand::Bool(false),
        });

        let head = builder.new_block();
        let body_block = builder.new_block();
        let exit = builder.new_block();
        builder.terminate(Terminator::Goto(head));
        builder.switch_to(head);
        let value = builder.local(Ty::Int, Some(variable));
        builder.emit(Instruction::RangeNext {
            has_value,
            value,
            next,
            end,
            step,
            done,
        });
        builder.terminate(Terminator::Branch {
            condition: Operand::Local(has_value),
            then_block: body_block,
            else_block: exit,
        });
        builder.switch_to(body_block);
        builder.scopes.push(HashMap::from([(variable.to_string(), Variable::Local(value))]));
        builder.loops.push((head, exit));
        let result = self.block(builder, body);
        builder.loops.pop();
        builder.scopes.pop();
        result?;
        builder.terminate(Terminator::Goto(head));
        builder.switch_to(exit);
        Ok(())
    }

    fn assign(
        &mut self,
        builder: &mut Builder,
        target: &AstNode,
        op: &AssignOperator,
        value: &AstNode,
    ) -> Lower<()> {
        let AstNode::Identifier(name) = target.unlocated() else {
            return self.unsupported("assigning to a field or an item");
        };
        let (place, target_type) = match builder.lookup(name) {
            Some(Variable::Local(id)) => (Place::Local(id), builder.locals[id].ty),
            Some(Variable::Global(id)) => (Place::Global(id), self.globals[id].ty),
            None => match self.global_names.get(name) {
                Some(&id) => (Place::Global(id), self.globals[id].ty),
                None => return self.unsupported("assigning to this variable"),
            },
        };
        let value_type = self.type_of(value)?;
        let operand = self.expr(builder, value)?;
        let result = match op.binary() {
            None => self.convert(builder, operand, value_type, target_type)?,
            Some(op) => {
                let current = match place {
                    Place::Local(id) => Operand::Local(id),
                    Place::Global(id) => Operand::Global(id),
                };
                let operand = operand.expect("a value");
                let (result, result_type) =
                    self.arithmetic(builder, op, current, target_type, operand, value_type)?;
                self.convert(builder, Some(result), result_type, target_type)?
            }
        };
        if let Some(value) = result {
            builder.emit(Instruction::Copy { dest: place, value });
        }
        Ok(())
    }

    // Expressions

    /// A `boolean` operand for a branch.
    fn condition(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<Operand> {
        Ok(self.expr(builder, node)?.expect("a boolean"))
    }

    /// Converts `value` from `from` to `to` (only `int` to `float` changes anything).
    fn convert(
        &mut self,
        builder: &mut Builder,
        value: Option<Operand>,
        from: Ty,
        to: Ty,
    ) -> Lower<Option<Operand>> {
        match (value, from, to) {
            (Some(value), Ty::Int, Ty::Float) => {
                let dest = builder.local(Ty::Float, None);
                builder.emit(Instruction::Unary {
                    dest,
                    op: UnaryOp::IntToFloat,
                    value,
                    line: self.line,
                });
                Ok(Some(Operand::Local(dest)))
            }
            (value, _, _) => Ok(value),
        }
    }

    /// The text of a value, as `print` and interpolation show it.
    fn display(&mut self, builder: &mut Builder, value: Operand, ty: Ty) -> Lower<Operand> {
        let op = match ty {
            Ty::Str => return Ok(value),
            Ty::Int => UnaryOp::IntToStr,
            Ty::Float => UnaryOp::FloatToStr,
            Ty::Bool => UnaryOp::BoolToStr,
            Ty::Void => return self.unsupported("showing 'void'"),
        };
        let dest = builder.local(Ty::Str, None);
        builder.emit(Instruction::Unary {
            dest,
            op,
            value,
            line: self.line,
        });
        Ok(Operand::Local(dest))
    }

    fn concat(&mut self, builder: &mut Builder, left: Operand, right: Operand) -> Operand {
        let dest = builder.local(Ty::Str, None);
        builder.emit(Instruction::Binary {
            dest,
            op: BinaryOp::StrConcat,
            left,
            right,
            line: self.line,
        });
        Operand::Local(dest)
    }

    /// Lowers an expression; `None` for an expression without a value (a call of a `void`
    /// function).
    fn expr(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<Option<Operand>> {
        match node {
            AstNode::Located { line, node } => {
                self.line = *line;
                self.expr(builder, node)
            }
            AstNode::IntLiteral(n) => Ok(Some(Operand::Int(*n))),
            AstNode::FloatLiteral(f) => Ok(Some(Operand::Float(*f))),
            AstNode::BooleanLiteral(b) => Ok(Some(Operand::Bool(*b))),
            AstNode::StringLiteral(text) => {
                if self.type_of(node)? != Ty::Str {
                    return self.unsupported("a char");
                }
                Ok(Some(self.string(text)))
            }
            AstNode::TemplateString(parts) => {
                let mut result: Option<Operand> = None;
                for part in parts {
                    let text = match part {
                        TemplatePart::Text(text) => self.string(text),
                        TemplatePart::Expression(expression, display) => {
                            if display.borrow().is_some() {
                                return self.unsupported("a 'display' method");
                            }
                            let ty = self.type_of(expression)?;
                            let value = self.expr(builder, expression)?.expect("a value");
                            self.display(builder, value, ty)?
                        }
                    };
                    result = Some(match result {
                        Some(left) => self.concat(builder, left, text),
                        None => text,
                    });
                }
                Ok(Some(result.unwrap_or_else(|| self.string(""))))
            }
            AstNode::Identifier(name) => match builder.lookup(name) {
                Some(Variable::Local(id)) => Ok(Some(Operand::Local(id))),
                Some(Variable::Global(id)) => Ok(Some(Operand::Global(id))),
                None => match self.global_names.get(name) {
                    Some(&id) => Ok(Some(Operand::Global(id))),
                    None if self.signatures.contains_key(name) => {
                        self.unsupported("a function used as a value")
                    }
                    None => self.unsupported(&format!("'{}'", name)),
                },
            },
            AstNode::Unary { op, operand } => {
                let ty = self.type_of(operand)?;
                let value = self.expr(builder, operand)?.expect("a value");
                let (op, result_type) = match (op, ty) {
                    (UnaryOperator::Negate, Ty::Int) => (UnaryOp::IntNeg, Ty::Int),
                    (UnaryOperator::Negate, Ty::Float) => (UnaryOp::FloatNeg, Ty::Float),
                    (UnaryOperator::Not, Ty::Bool) => (UnaryOp::Not, Ty::Bool),
                    (UnaryOperator::BitNot, Ty::Int) => (UnaryOp::BitNot, Ty::Int),
                    _ => return self.unsupported("this operator"),
                };
                let dest = builder.local(result_type, None);
                builder.emit(Instruction::Unary {
                    dest,
                    op,
                    value,
                    line: self.line,
                });
                Ok(Some(Operand::Local(dest)))
            }
            AstNode::BinaryOp { left, op, right } => match op {
                BinaryOperator::And | BinaryOperator::Or => {
                    self.short_circuit(builder, left, *op, right).map(Some)
                }
                BinaryOperator::Is | BinaryOperator::In => {
                    self.unsupported(&format!("'{}'", op.symbol()))
                }
                _ => {
                    let left_type = self.type_of(left)?;
                    let left_value = self.expr(builder, left)?.expect("a value");
                    let right_type = self.type_of(right)?;
                    let right_value = self.expr(builder, right)?.expect("a value");
                    let (result, _) = self.arithmetic(
                        builder,
                        *op,
                        left_value,
                        left_type,
                        right_value,
                        right_type,
                    )?;
                    Ok(Some(result))
                }
            },
            AstNode::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
            } => {
                let ty = self.type_of(node)?;
                let dest = builder.local(ty, None);
                let condition = self.condition(builder, condition)?;
                let then_block = builder.new_block();
                let else_block = builder.new_block();
                let exit = builder.new_block();
                builder.terminate(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                });
                for (block, branch) in [(then_block, then_branch), (else_block, else_branch)] {
                    builder.switch_to(block);
                    let value = self.branch_value(builder, branch)?;
                    let value = self.convert(builder, value, self.branch_type(branch)?, ty)?;
                    if let Some(value) = value {
                        builder.emit(Instruction::Copy {
                            dest: Place::Local(dest),
                            value,
                        });
                    }
                    builder.terminate(Terminator::Goto(exit));
                }
                builder.switch_to(exit);
                Ok(Some(Operand::Local(dest)))
            }
            AstNode::Call {
                callee,
                args,
                method,
                displays,
            } => {
                if method.borrow().is_some() {
                    return self.unsupported("a method call");
                }
                let AstNode::Identifier(name) = callee.unlocated() else {
                    return self.unsupported("this call");
                };
                if builder.lookup(name).is_some() || self.global_names.contains_key(name) {
                    return self.unsupported("calling a function stored in a variable");
                }
                if displays.borrow().iter().any(Option::is_some) {
                    return self.unsupported("a 'display' method");
                }
                self.call(builder, name, args)
            }
            _ => self.unsupported(describe(node)),
        }
    }

    /// The value of an `if` branch used as an expression: the last expression of its block.
    fn branch_value(&mut self, builder: &mut Builder, branch: &AstNode) -> Lower<Option<Operand>> {
        match branch.unlocated() {
            AstNode::Block(statements) => {
                let Some((last, rest)) = statements.split_last() else {
                    return Ok(None);
                };
                builder.scopes.push(HashMap::new());
                let result = self
                    .statements(builder, rest)
                    .and_then(|_| self.expr(builder, last));
                builder.scopes.pop();
                result
            }
            other => self.expr(builder, other),
        }
    }

    fn branch_type(&self, branch: &AstNode) -> Lower<Ty> {
        match branch.unlocated() {
            AstNode::Block(statements) => match statements.last() {
                Some(last) => self.type_of(last),
                None => Ok(Ty::Void),
            },
            other => self.type_of(other),
        }
    }

    /// `left and right`, `left or right`: `right` only runs when needed.
    fn short_circuit(
        &mut self,
        builder: &mut Builder,
        left: &AstNode,
        op: BinaryOperator,
        right: &AstNode,
    ) -> Lower<Operand> {
        let dest = builder.local(Ty::Bool, None);
        let left = self.condition(builder, left)?;
        builder.emit(Instruction::Copy {
            dest: Place::Local(dest),
            value: left,
        });
        let right_block = builder.new_block();
        let exit = builder.new_block();
        let (then_block, else_block) = if op == BinaryOperator::And {
            (right_block, exit)
        } else {
            (exit, right_block)
        };
        builder.terminate(Terminator::Branch {
            condition: Operand::Local(dest),
            then_block,
            else_block,
        });
        builder.switch_to(right_block);
        let right = self.condition(builder, right)?;
        builder.emit(Instruction::Copy {
            dest: Place::Local(dest),
            value: right,
        });
        builder.terminate(Terminator::Goto(exit));
        builder.switch_to(exit);
        Ok(Operand::Local(dest))
    }

    /// A binary operator on two values; returns the result and its type.
    fn arithmetic(
        &mut self,
        builder: &mut Builder,
        op: BinaryOperator,
        left: Operand,
        left_type: Ty,
        right: Operand,
        right_type: Ty,
    ) -> Lower<(Operand, Ty)> {
        use BinaryOperator as B;
        let comparison = match op {
            B::Equal => Some(Comparison::Equal),
            B::NotEqual => Some(Comparison::NotEqual),
            B::Less => Some(Comparison::Less),
            B::LessEqual => Some(Comparison::LessEqual),
            B::Greater => Some(Comparison::Greater),
            B::GreaterEqual => Some(Comparison::GreaterEqual),
            _ => None,
        };
        let numeric = |ty: Ty| matches!(ty, Ty::Int | Ty::Float);
        let (op, left, right, result_type) = match (left_type, right_type) {
            (Ty::Int, Ty::Int) => {
                let op = match (op, comparison) {
                    (_, Some(comparison)) => BinaryOp::IntCompare(comparison),
                    (B::Add, _) => BinaryOp::IntAdd,
                    (B::Subtract, _) => BinaryOp::IntSub,
                    (B::Multiply, _) => BinaryOp::IntMul,
                    (B::Divide, _) => BinaryOp::IntDiv,
                    (B::Modulo, _) => BinaryOp::IntRem,
                    (B::Power, _) => BinaryOp::IntPow,
                    (B::BitAnd, _) => BinaryOp::BitAnd,
                    (B::BitOr, _) => BinaryOp::BitOr,
                    (B::BitXor, _) => BinaryOp::BitXor,
                    (B::ShiftLeft, _) => BinaryOp::ShiftLeft,
                    (B::ShiftRight, _) => BinaryOp::ShiftRight,
                    _ => return self.unsupported(&format!("'{}'", op.symbol())),
                };
                let ty = if comparison.is_some() { Ty::Bool } else { Ty::Int };
                (op, left, right, ty)
            }
            (l, r) if numeric(l) && numeric(r) => {
                let left = self.convert(builder, Some(left), l, Ty::Float)?.expect("a value");
                let right = self.convert(builder, Some(right), r, Ty::Float)?.expect("a value");
                let op = match (op, comparison) {
                    (_, Some(comparison)) => BinaryOp::FloatCompare(comparison),
                    (B::Add, _) => BinaryOp::FloatAdd,
                    (B::Subtract, _) => BinaryOp::FloatSub,
                    (B::Multiply, _) => BinaryOp::FloatMul,
                    (B::Divide, _) => BinaryOp::FloatDiv,
                    (B::Modulo, _) => BinaryOp::FloatRem,
                    (B::Power, _) => BinaryOp::FloatPow,
                    _ => return self.unsupported(&format!("'{}' on floats", op.symbol())),
                };
                let ty = if comparison.is_some() { Ty::Bool } else { Ty::Float };
                (op, left, right, ty)
            }
            (Ty::Str, Ty::Str) => match (op, comparison) {
                (_, Some(comparison)) => (BinaryOp::StrCompare(comparison), left, right, Ty::Bool),
                (B::Add, _) => (BinaryOp::StrConcat, left, right, Ty::Str),
                _ => return self.unsupported(&format!("'{}' on strings", op.symbol())),
            },
            (Ty::Str, Ty::Int) if op == B::Multiply => (BinaryOp::StrRepeat, left, right, Ty::Str),
            (Ty::Int, Ty::Str) if op == B::Multiply => (BinaryOp::StrRepeat, right, left, Ty::Str),
            (Ty::Bool, Ty::Bool) => match comparison {
                Some(comparison @ (Comparison::Equal | Comparison::NotEqual)) => {
                    (BinaryOp::BoolCompare(comparison), left, right, Ty::Bool)
                }
                _ => return self.unsupported(&format!("'{}' on booleans", op.symbol())),
            },
            _ => return self.unsupported(&format!("'{}' on these types", op.symbol())),
        };
        let dest = builder.local(result_type, None);
        builder.emit(Instruction::Binary {
            dest,
            op,
            left,
            right,
            line: self.line,
        });
        Ok((Operand::Local(dest), result_type))
    }

    fn call(&mut self, builder: &mut Builder, name: &str, args: &[AstNode]) -> Lower<Option<Operand>> {
        if let Some(signature) = self.signatures.get(name) {
            let (id, params, ret) = (signature.id, signature.params.clone(), signature.ret);
            let mut values = Vec::new();
            for (arg, param) in args.iter().zip(params) {
                let ty = self.type_of(arg)?;
                let value = self.expr(builder, arg)?;
                values.push(self.convert(builder, value, ty, param)?.expect("an argument"));
            }
            let dest = (ret != Ty::Void).then(|| builder.local(ret, None));
            builder.emit(Instruction::Call {
                dest,
                function: id,
                args: values,
                line: self.line,
            });
            return Ok(dest.map(Operand::Local));
        }
        match name {
            "print" => {
                let mut line: Option<Operand> = None;
                for arg in args {
                    let ty = self.type_of(arg)?;
                    let value = self.expr(builder, arg)?.expect("a value");
                    let text = self.display(builder, value, ty)?;
                    line = Some(match line {
                        Some(left) => {
                            let space = self.string(" ");
                            let with_space = self.concat(builder, left, space);
                            self.concat(builder, with_space, text)
                        }
                        None => text,
                    });
                }
                let value = line.unwrap_or_else(|| self.string(""));
                builder.emit(Instruction::Print {
                    value,
                    line: self.line,
                });
                Ok(None)
            }
            "string" if args.len() == 1 => {
                let ty = self.type_of(&args[0])?;
                let value = self.expr(builder, &args[0])?.expect("a value");
                self.display(builder, value, ty).map(Some)
            }
            "float" if args.len() == 1 && self.type_of(&args[0])? != Ty::Str => {
                let ty = self.type_of(&args[0])?;
                let value = self.expr(builder, &args[0])?;
                if ty == Ty::Bool {
                    return self.unsupported("'float' of a boolean");
                }
                self.convert(builder, value, ty, Ty::Float)
            }
            _ => self.unsupported(&format!("'{}'", name)),
        }
    }
}

/// What an unsupported expression is, for the error message.
fn describe(node: &AstNode) -> &'static str {
    match node {
        AstNode::ArrayLiteral(_) => "a list",
        AstNode::MapLiteral(_) => "a map",
        AstNode::ObjectLiteral(_) => "an object",
        AstNode::Index { .. } => "indexing",
        AstNode::Slice { .. } => "a slice",
        AstNode::Member { .. } => "a member access",
        AstNode::Match { .. } => "'match'",
        AstNode::Lambda { .. } => "an anonymous function",
        AstNode::Try { .. } => "'?'",
        AstNode::MutArg(_) => "a 'mut' argument",
        AstNode::If { .. } => "this 'if'",
        _ => "this expression",
    }
}
