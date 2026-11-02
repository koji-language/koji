//! Lowering: from the checked AST to the IR.
//!
//! The types come from the type checker (`Ownership::types` and `Ownership::variable_types`);
//! this pass only chooses the typed operations, makes conversions explicit (an `int` passed
//! where a `float` is expected) and turns control flow into basic blocks. A construct the
//! compiler does not support yet is a `CompileError` that names it.

use std::collections::HashMap;
use std::path::Path;

use super::*;
use crate::error::EzError;
use crate::parser::ast::{
    AssignOperator, AstNode, BinaryOperator, TemplatePart, TypeExpr, UnaryOperator,
};
use crate::semantic::type_checker::{node_id, Program as CheckedProgram};
use crate::semantic::types::{Type, TypeId, TypeKind, TypeTable};

type Lower<T> = Result<T, EzError>;

/// Lowers the main file of a checked program.
pub fn lower_program(program: &CheckedProgram) -> Lower<Program> {
    let statements = &program.modules[&program.entry];
    let file = display_path(&program.entry);
    let mut lowerer = Lowerer {
        table: &program.table,
        records: Vec::new(),
        record_ids: HashMap::new(),
        types: &program.ownership.types,
        variable_types: &program.ownership.variable_types,
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
        records: lowerer.records,
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
    table: &'a TypeTable,
    records: Vec<Record>,
    /// The record of each declared type with its type arguments.
    record_ids: HashMap<(TypeId, Vec<Ty>), RecordId>,
    types: &'a HashMap<usize, Type>,
    variable_types: &'a HashMap<usize, Type>,
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

    fn declare(&mut self, name: &str, variable: Variable) {
        self.scopes
            .last_mut()
            .expect("a scope")
            .insert(name.to_string(), variable);
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

    fn ty(&mut self, ty: &Type) -> Lower<Ty> {
        match ty {
            Type::Int => Ok(Ty::Int),
            Type::Float => Ok(Ty::Float),
            Type::Boolean => Ok(Ty::Bool),
            Type::String => Ok(Ty::Str),
            Type::Void | Type::Never => Ok(Ty::Void),
            Type::List(element) => Ok(Ty::List(Box::new(self.ty(element)?))),
            Type::Char => self.unsupported("the type 'char'"),
            Type::Function { .. } => self.unsupported("a function value"),
            Type::Named { id, args } | Type::Variant { id, args, .. } => self.record(*id, args).map(Ty::Record),
            Type::Unknown => self.unsupported("a value whose type is not known here"),
            _ => self.unsupported("this type"),
        }
    }

    /// The record of the declared type `id` with the type arguments `args`.
    fn record(&mut self, id: TypeId, args: &[Type]) -> Lower<RecordId> {
        let mut arg_types = Vec::new();
        for arg in args {
            arg_types.push(self.ty(arg)?);
        }
        let key = (id, arg_types);
        if let Some(record) = self.record_ids.get(&key) {
            return Ok(*record);
        }
        let info = self.table.get(id);
        let declared = match &info.kind {
            TypeKind::Sum(variants) => variants
                .iter()
                .map(|variant| (Some(variant.name.clone()), variant.fields.clone()))
                .collect::<Vec<_>>(),
            TypeKind::Shape(fields) => vec![(None, fields.clone())],
            TypeKind::Map { .. } => return self.unsupported("a Map"),
            TypeKind::Set { .. } => return self.unsupported("a Set"),
            _ => return self.unsupported(&format!("the type '{}'", info.name)),
        };
        // Registered before its fields: a type can contain itself (`tail: List`).
        let record = self.records.len();
        self.records.push(Record {
            name: self.table.display(&Type::Named {
                id,
                args: args.to_vec(),
            }),
            variants: Vec::new(),
        });
        self.record_ids.insert(key, record);
        let mut variants = Vec::new();
        for (name, fields) in declared {
            let mut lowered = Vec::new();
            for (field, ty) in fields {
                lowered.push((field, self.ty(&ty.substitute(args))?));
            }
            variants.push(Variant {
                name,
                fields: lowered,
            });
        }
        self.records[record].variants = variants;
        Ok(record)
    }

    fn type_expr(&mut self, ty: &TypeExpr) -> Lower<Ty> {
        match ty {
            TypeExpr::Named(name) => match name.as_str() {
                "int" => Ok(Ty::Int),
                "float" => Ok(Ty::Float),
                "boolean" => Ok(Ty::Bool),
                "string" => Ok(Ty::Str),
                "void" => Ok(Ty::Void),
                _ => self.named_type(name, &[]),
            },
            TypeExpr::Generic(name, args) => self.named_type(name, args),
            TypeExpr::Array(element) => Ok(Ty::List(Box::new(self.type_expr(element)?))),
            _ => self.unsupported("this type"),
        }
    }

    /// A declared type written in the source (`Person`, `Option[int]`).
    fn named_type(&mut self, name: &str, args: &[TypeExpr]) -> Lower<Ty> {
        let Some(id) = self.table.types.iter().position(|info| info.name == name) else {
            return self.unsupported(&format!("the type '{}'", name));
        };
        if self.table.types.iter().filter(|info| info.name == name).count() > 1 {
            return self.unsupported(&format!("the type '{}' (declared more than once)", name));
        }
        let mut types = Vec::new();
        for arg in args {
            types.push(self.checked_type(arg)?);
        }
        self.record(id, &types).map(Ty::Record)
    }

    /// The checker's type for a written type argument.
    fn checked_type(&mut self, ty: &TypeExpr) -> Lower<Type> {
        Ok(match ty {
            TypeExpr::Named(name) => match name.as_str() {
                "int" => Type::Int,
                "float" => Type::Float,
                "boolean" => Type::Boolean,
                "string" => Type::String,
                _ => match self.table.types.iter().position(|info| info.name == *name) {
                    Some(id) => Type::Named { id, args: Vec::new() },
                    None => return self.unsupported(&format!("the type '{}'", name)),
                },
            },
            TypeExpr::Generic(name, args) => {
                let Some(id) = self.table.types.iter().position(|info| info.name == *name) else {
                    return self.unsupported(&format!("the type '{}'", name));
                };
                let mut types = Vec::new();
                for arg in args {
                    types.push(self.checked_type(arg)?);
                }
                Type::Named { id, args: types }
            }
            TypeExpr::Array(element) => Type::List(Box::new(self.checked_type(element)?)),
            _ => return self.unsupported("this type"),
        })
    }

    /// The type of an expression, as found by the checker.
    fn type_of(&mut self, node: &AstNode) -> Lower<Ty> {
        match self.types.get(&node_id(node)) {
            Some(ty) => self.ty(ty),
            None => self.unsupported("this expression"),
        }
    }

    /// The type of an expression in a function: a variable has the type found for it at its
    /// declaration, which later uses may have refined (`mut items = []` then `items.push(1)`).
    fn type_in(&mut self, builder: &Builder, node: &AstNode) -> Lower<Ty> {
        if let AstNode::Identifier(name) = node.unlocated() {
            if let Ok((_, ty)) = self.variable(builder, name) {
                return Ok(ty);
            }
        }
        self.type_of(node)
    }

    /// The type of a declared variable, after the uses that refine it (`mut items = []` then
    /// `items.push(1)`).
    fn variable_type(&mut self, declaration: &AstNode, annotation: &Option<TypeExpr>, value: &AstNode) -> Lower<Ty> {
        if let Some(ty) = annotation {
            return self.type_expr(ty);
        }
        match self.variable_types.get(&node_id(declaration)) {
            Some(ty) => self.ty(&ty.clone()),
            None => self.type_of(value),
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
            let declaration = statement.unlocated();
            if let AstNode::VarDecl { name, ty, value, .. } = declaration {
                let ty = self.variable_type(declaration, ty, value)?;
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
                let (id, param_types, ret) = (signature.id, signature.params.clone(), signature.ret.clone());
                let mut builder = Builder::new(ret.clone(), false);
                for (param, ty) in params.iter().zip(param_types) {
                    let local = builder.local(ty, Some(&param.name));
                    builder.declare(&param.name, Variable::Local(local));
                }
                builder.scopes.push(HashMap::new());
                let last_gives_value = match body.unlocated() {
                    AstNode::Block(statements) => match statements.last() {
                        Some(last) if is_expression(last) => !matches!(self.type_of(last), Ok(Ty::Void) | Err(_)),
                        _ => false,
                    },
                    _ => false,
                };
                match body.unlocated() {
                    // The value of the last expression is returned (`function area(...): float {
                    // match (shape) { ... } }`).
                    AstNode::Block(statements) if ret != Ty::Void && last_gives_value => {
                        let (last, rest) = statements.split_last().expect("a statement");
                        self.statements(&mut builder, rest)?;
                        let value = self.value_as(&mut builder, last, &ret)?;
                        builder.terminate(Terminator::Return(value));
                    }
                    AstNode::Block(statements) => self.statements(&mut builder, statements)?,
                    expression => {
                        let value = self.value_as(&mut builder, expression, &ret)?;
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
            AstNode::TypeDecl { .. } if builder.is_main && builder.scopes.len() == 1 => Ok(()),
            AstNode::TypeDecl { .. } => self.unsupported("a type declared inside a block"),
            AstNode::Import { .. } => self.unsupported("an import"),
            AstNode::VarDecl { name, ty, value, .. } => {
                let var_type = self.variable_type(node, ty, value)?;
                let operand = self.value_as(builder, value, &var_type)?;
                let place = if builder.is_main && builder.scopes.len() == 1 {
                    let id = self.global_names[name];
                    builder.declare(name, Variable::Global(id));
                    Place::Global(id)
                } else {
                    Place::Local(builder.local(var_type, Some(name)))
                };
                if let Some(value) = operand {
                    builder.emit(Instruction::Copy { dest: place, value });
                }
                // Declared after its value: `immut x = x + 1` reads the outer `x`.
                if let Place::Local(id) = place {
                    builder.declare(name, Variable::Local(id));
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
            } => {
                if self.is_range_call(builder, iterable) {
                    self.for_range(builder, variable, iterable, body)
                } else {
                    self.for_list(builder, variable, iterable, body)
                }
            }
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
                        let ret = builder.ret.clone();
                        self.value_as(builder, value, &ret)?
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

    /// Whether `iterable` is a call of the built-in `range`.
    fn is_range_call(&self, builder: &Builder, iterable: &AstNode) -> bool {
        matches!(iterable.unlocated(), AstNode::Call { callee, .. }
            if matches!(callee.unlocated(), AstNode::Identifier(name) if name == "range")
                && builder.lookup("range").is_none()
                && !self.signatures.contains_key("range")
                && !self.global_names.contains_key("range"))
    }

    /// `for i in range(...)`, without building the list.
    fn for_range(
        &mut self,
        builder: &mut Builder,
        variable: &str,
        iterable: &AstNode,
        body: &AstNode,
    ) -> Lower<()> {
        let AstNode::Call { args, .. } = iterable.unlocated() else {
            unreachable!("checked by is_range_call");
        };
        let mut values = Vec::new();
        for arg in args {
            let value = self.value_as(builder, arg, &Ty::Int)?.expect("an int");
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
        self.loop_body(builder, variable, value, head, exit, body)
    }

    /// The body of a `for` loop, with `variable` in scope; then back to `next`.
    fn loop_body(
        &mut self,
        builder: &mut Builder,
        variable: &str,
        value: LocalId,
        next: BlockId,
        exit: BlockId,
        body: &AstNode,
    ) -> Lower<()> {
        builder.scopes.push(HashMap::from([(variable.to_string(), Variable::Local(value))]));
        builder.loops.push((next, exit));
        let result = self.block(builder, body);
        builder.loops.pop();
        builder.scopes.pop();
        result?;
        builder.terminate(Terminator::Goto(next));
        builder.switch_to(exit);
        Ok(())
    }

    /// `for item in items`: goes through a copy of the list, so changing `items` in the body
    /// does not change the loop.
    fn for_list(
        &mut self,
        builder: &mut Builder,
        variable: &str,
        iterable: &AstNode,
        body: &AstNode,
    ) -> Lower<()> {
        let list_type = self.type_in(builder, iterable)?;
        let Ty::List(element) = &list_type else {
            return self.unsupported("a 'for' loop over a string (char)");
        };
        let element = (**element).clone();
        let value = self.expr(builder, iterable)?.expect("a list");
        let list = builder.local(list_type.clone(), None);
        builder.emit(Instruction::Copy {
            dest: Place::Local(list),
            value,
        });
        let length = builder.local(Ty::Int, None);
        builder.emit(Instruction::Unary {
            dest: length,
            op: UnaryOp::ListLength,
            value: Operand::Local(list),
            line: self.line,
        });
        let index = builder.local(Ty::Int, None);
        builder.emit(Instruction::Copy {
            dest: Place::Local(index),
            value: Operand::Int(0),
        });

        let head = builder.new_block();
        let body_block = builder.new_block();
        let step = builder.new_block();
        let exit = builder.new_block();
        builder.terminate(Terminator::Goto(head));
        builder.switch_to(head);
        let more = builder.local(Ty::Bool, None);
        builder.emit(Instruction::Binary {
            dest: more,
            op: BinaryOp::IntCompare(Comparison::Less),
            left: Operand::Local(index),
            right: Operand::Local(length),
            line: self.line,
        });
        builder.terminate(Terminator::Branch {
            condition: Operand::Local(more),
            then_block: body_block,
            else_block: exit,
        });

        builder.switch_to(step);
        let following = builder.local(Ty::Int, None);
        builder.emit(Instruction::Binary {
            dest: following,
            op: BinaryOp::IntAdd,
            left: Operand::Local(index),
            right: Operand::Int(1),
            line: self.line,
        });
        builder.emit(Instruction::Copy {
            dest: Place::Local(index),
            value: Operand::Local(following),
        });
        builder.terminate(Terminator::Goto(head));

        builder.switch_to(body_block);
        let item = builder.local(element, Some(variable));
        builder.emit(Instruction::ListGet {
            dest: item,
            list: Operand::Local(list),
            index: Operand::Local(index),
            line: self.line,
        });
        self.loop_body(builder, variable, item, step, exit, body)
    }

    /// The variable `name`.
    fn variable(&self, builder: &Builder, name: &str) -> Lower<(Place, Ty)> {
        let variable = builder
            .lookup(name)
            .or_else(|| self.global_names.get(name).map(|id| Variable::Global(*id)));
        match variable {
            Some(Variable::Local(id)) => Ok((Place::Local(id), builder.locals[id].ty.clone())),
            Some(Variable::Global(id)) => Ok((Place::Global(id), self.globals[id].ty.clone())),
            None => self.unsupported(&format!("'{}'", name)),
        }
    }

    /// The type of an assignable place: a variable or an item of a list.
    fn place_type(&mut self, builder: &Builder, node: &AstNode) -> Lower<Ty> {
        match node.unlocated() {
            AstNode::Identifier(name) => Ok(self.variable(builder, name)?.1),
            AstNode::Index { target, .. } => match self.place_type(builder, target)? {
                Ty::List(element) => Ok(*element),
                _ => self.unsupported("assigning to an item of this value"),
            },
            AstNode::Member { object, name } => Ok(self.field(builder, object, name)?.1),
            _ => self.unsupported("assigning to this value"),
        }
    }

    /// The position and type of the field `name` of `object` (an object, or a variant the
    /// checker narrowed it to: `if (result is Ok) { result.value }`).
    fn field(&mut self, builder: &Builder, object: &AstNode, name: &str) -> Lower<(usize, Ty)> {
        let narrowed = match self.types.get(&node_id(object)) {
            Some(Type::Variant { variant, .. }) => Some(variant.clone()),
            _ => None,
        };
        let record = match self.type_in(builder, object)? {
            Ty::Record(record) => record,
            _ => return self.unsupported("this member"),
        };
        let variants = &self.records[record].variants;
        let variant = match narrowed {
            Some(narrowed) => variants.iter().find(|v| v.name.as_deref() == Some(&narrowed)),
            None if variants.len() == 1 => variants.first(),
            None => None,
        };
        match variant.and_then(|v| v.fields.iter().position(|(field, _)| field == name).map(|i| (i, v.fields[i].1.clone()))) {
            Some(field) => Ok(field),
            None => self.unsupported(&format!("the member '{}'", name)),
        }
    }

    /// `Some(1)`, `None` or `Circle(2.0)` when `node` constructs a variant of `record`: a name
    /// that is not a variable or a function.
    fn constructor<'n>(&self, builder: &Builder, node: &'n AstNode, record: RecordId) -> Option<(&'n str, &'n [AstNode])> {
        let (name, args): (&str, &[AstNode]) = match node.unlocated() {
            AstNode::Identifier(name) => (name, &[]),
            AstNode::Call { callee, args, method, .. } if method.borrow().is_none() => match callee.unlocated() {
                AstNode::Identifier(name) => (name, args),
                _ => return None,
            },
            _ => return None,
        };
        let is_name = builder.lookup(name).is_some()
            || self.global_names.contains_key(name)
            || self.signatures.contains_key(name);
        let is_variant = self.records[record]
            .variants
            .iter()
            .any(|variant| variant.name.as_deref() == Some(name));
        (!is_name && is_variant).then_some((name, args))
    }

    /// A new value of the variant `name` of `record`.
    fn construct(&mut self, builder: &mut Builder, record: RecordId, name: &str, args: &[AstNode]) -> Lower<Operand> {
        let variant = self.records[record]
            .variants
            .iter()
            .position(|variant| variant.name.as_deref() == Some(name))
            .expect("a variant of the record");
        let fields: Vec<Ty> = self.records[record].variants[variant].fields.iter().map(|(_, ty)| ty.clone()).collect();
        let mut values = Vec::new();
        for (arg, ty) in args.iter().zip(&fields) {
            values.push(self.value_as(builder, arg, ty)?.expect("a field"));
        }
        let dest = builder.local(Ty::Record(record), None);
        builder.emit(Instruction::RecordNew { dest, variant, fields: values });
        Ok(Operand::Local(dest))
    }

    /// `{ name: 'Ana', age: 30 }`: the fields are evaluated in the order they are written, and
    /// stored in the order of the type.
    fn object_literal(
        &mut self,
        builder: &mut Builder,
        entries: &[crate::parser::ast::ObjectEntry],
        record: RecordId,
    ) -> Lower<Operand> {
        let fields = self.records[record].variants[0].fields.clone();
        let mut values: Vec<Option<Operand>> = vec![None; fields.len()];
        for entry in entries {
            let Some(index) = fields.iter().position(|(name, _)| *name == entry.key) else {
                return self.unsupported(&format!("the field '{}'", entry.key));
            };
            let value = self.value_as(builder, &entry.value, &fields[index].1)?.expect("a field");
            // Kept in a local: a later field could change the variable it reads.
            let local = builder.local(fields[index].1.clone(), None);
            builder.emit(Instruction::Copy { dest: Place::Local(local), value });
            values[index] = Some(Operand::Local(local));
        }
        let dest = builder.local(Ty::Record(record), None);
        builder.emit(Instruction::RecordNew {
            dest,
            variant: 0,
            fields: values.into_iter().map(|value| value.expect("every field")).collect(),
        });
        Ok(Operand::Local(dest))
    }

    /// A list that is changed in place: a variable, or an item of a list (`grid[0]`).
    fn target(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<Target> {
        match node {
            AstNode::Located { line, node } => {
                self.line = *line;
                self.target(builder, node)
            }
            AstNode::Identifier(name) => Ok(Target {
                root: self.variable(builder, name)?.0,
                path: Vec::new(),
            }),
            AstNode::Index { target, index } => {
                let mut list = self.target(builder, target)?;
                list.path.push(Step::Index(self.index(builder, index)?));
                Ok(list)
            }
            AstNode::Member { object, name } => {
                let (field, _) = self.field(builder, object, name)?;
                let mut record = self.target(builder, object)?;
                record.path.push(Step::Field(field));
                Ok(record)
            }
            _ => self.unsupported("changing this value"),
        }
    }

    /// An `int` index, evaluated once.
    fn index(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<Operand> {
        let value = self.value_as(builder, node, &Ty::Int)?.expect("an int");
        if matches!(value, Operand::Int(_)) {
            return Ok(value);
        }
        let local = builder.local(Ty::Int, None);
        builder.emit(Instruction::Copy {
            dest: Place::Local(local),
            value,
        });
        Ok(Operand::Local(local))
    }

    fn assign(
        &mut self,
        builder: &mut Builder,
        target: &AstNode,
        op: &AssignOperator,
        value: &AstNode,
    ) -> Lower<()> {
        let target_type = self.place_type(builder, target)?;
        let new_value = |lowerer: &mut Self, builder: &mut Builder| -> Lower<Option<Operand>> {
            match op.binary() {
                None => lowerer.value_as(builder, value, &target_type),
                Some(op) => {
                    let current = lowerer.expr(builder, target)?.expect("a value");
                    let value_type = lowerer.type_in(builder, value)?;
                    let operand = lowerer.expr(builder, value)?.expect("a value");
                    let (result, result_type) =
                        lowerer.arithmetic(builder, op, current, &target_type, operand, &value_type)?;
                    lowerer.convert(builder, Some(result), &result_type, &target_type)
                }
            }
        };
        match target.unlocated() {
            AstNode::Identifier(name) => {
                let (place, _) = self.variable(builder, name)?;
                if let Some(value) = new_value(self, builder)? {
                    builder.emit(Instruction::Copy { dest: place, value });
                }
                Ok(())
            }
            AstNode::Index { target: list, index } => {
                let list = self.target(builder, list)?;
                let index = self.index(builder, index)?;
                let value = new_value(self, builder)?.expect("a value");
                builder.emit(Instruction::ListSet {
                    list,
                    index,
                    value,
                    line: self.line,
                });
                Ok(())
            }
            AstNode::Member { object, name } => {
                let (field, _) = self.field(builder, object, name)?;
                let record = self.target(builder, object)?;
                let value = new_value(self, builder)?.expect("a value");
                builder.emit(Instruction::FieldSet { record, field, value, line: self.line });
                Ok(())
            }
            _ => self.unsupported("assigning to this value"),
        }
    }

    // Expressions

    /// A `boolean` operand for a branch.
    fn condition(&mut self, builder: &mut Builder, node: &AstNode) -> Lower<Operand> {
        Ok(self.expr(builder, node)?.expect("a boolean"))
    }

    /// Lowers `node` as a value of type `target`: an `int` becomes a `float` where a `float`
    /// is expected, and a list literal gets its type from `target` (`[]`, `[1, 2]` for a
    /// `float[]`).
    fn value_as(&mut self, builder: &mut Builder, node: &AstNode, target: &Ty) -> Lower<Option<Operand>> {
        match node {
            AstNode::Located { line, node } => {
                self.line = *line;
                self.value_as(builder, node, target)
            }
            AstNode::ArrayLiteral(items) if matches!(target, Ty::List(_)) => {
                self.list_literal(builder, items, target).map(Some)
            }
            AstNode::ObjectLiteral(entries) if matches!(target, Ty::Record(_)) => {
                let Ty::Record(record) = target else { unreachable!() };
                self.object_literal(builder, entries, *record).map(Some)
            }
            _ if matches!(target, Ty::Record(record) if self.constructor(builder, node, *record).is_some()) => {
                let Ty::Record(record) = target else { unreachable!() };
                let (name, args) = self.constructor(builder, node, *record).expect("a constructor");
                self.construct(builder, *record, name, args).map(Some)
            }
            _ => {
                let from = self.type_in(builder, node)?;
                let value = self.expr(builder, node)?;
                self.convert(builder, value, &from, target)
            }
        }
    }

    fn list_literal(&mut self, builder: &mut Builder, items: &[AstNode], ty: &Ty) -> Lower<Operand> {
        let element = ty.element().clone();
        let mut values = Vec::new();
        for item in items {
            values.push(self.value_as(builder, item, &element)?.expect("an item"));
        }
        let dest = builder.local(ty.clone(), None);
        builder.emit(Instruction::ListNew { dest, items: values });
        Ok(Operand::Local(dest))
    }

    /// Converts `value` from `from` to `to` (only `int` to `float` changes anything).
    fn convert(
        &mut self,
        builder: &mut Builder,
        value: Option<Operand>,
        from: &Ty,
        to: &Ty,
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
            (Some(_), Ty::List(_), Ty::List(_)) if from != to => {
                self.unsupported("converting a list to another type of list")
            }
            (value, _, _) => Ok(value),
        }
    }

    /// The text of a value, as `print` and interpolation show it.
    fn display(&mut self, builder: &mut Builder, value: Operand, ty: &Ty) -> Lower<Operand> {
        let op = match ty {
            Ty::Str => return Ok(value),
            Ty::Int => UnaryOp::IntToStr,
            Ty::Float => UnaryOp::FloatToStr,
            Ty::Bool => UnaryOp::BoolToStr,
            Ty::List(_) => UnaryOp::ListToStr,
            Ty::Record(_) => UnaryOp::RecordToStr,
            Ty::Void => return self.unsupported("showing 'void'"),
        };
        Ok(self.unary(builder, op, value, Ty::Str))
    }

    fn unary(&mut self, builder: &mut Builder, op: UnaryOp, value: Operand, ty: Ty) -> Operand {
        let dest = builder.local(ty, None);
        builder.emit(Instruction::Unary {
            dest,
            op,
            value,
            line: self.line,
        });
        Operand::Local(dest)
    }

    fn binary(&mut self, builder: &mut Builder, op: BinaryOp, left: Operand, right: Operand, ty: Ty) -> Operand {
        let dest = builder.local(ty, None);
        builder.emit(Instruction::Binary {
            dest,
            op,
            left,
            right,
            line: self.line,
        });
        Operand::Local(dest)
    }

    fn concat(&mut self, builder: &mut Builder, left: Operand, right: Operand) -> Operand {
        self.binary(builder, BinaryOp::StrConcat, left, right, Ty::Str)
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
                if self.type_in(builder, node)? != Ty::Str {
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
                            let ty = self.type_in(builder, expression)?;
                            let value = self.expr(builder, expression)?.expect("a value");
                            self.display(builder, value, &ty)?
                        }
                    };
                    result = Some(match result {
                        Some(left) => self.concat(builder, left, text),
                        None => text,
                    });
                }
                Ok(Some(result.unwrap_or_else(|| self.string(""))))
            }
            AstNode::ArrayLiteral(items) => {
                let ty = self.type_in(builder, node)?;
                self.list_literal(builder, items, &ty).map(Some)
            }
            AstNode::ObjectLiteral(entries) => {
                let Ty::Record(record) = self.type_in(builder, node)? else {
                    return self.unsupported("an object of this type");
                };
                self.object_literal(builder, entries, record).map(Some)
            }
            AstNode::Member { object, name } => {
                let (field, ty) = self.field(builder, object, name)?;
                let record = self.expr(builder, object)?.expect("a record");
                let dest = builder.local(ty, None);
                builder.emit(Instruction::FieldGet { dest, record, field });
                Ok(Some(Operand::Local(dest)))
            }
            AstNode::Match { scrutinee, arms } => self.lower_match(builder, node, scrutinee, arms),
            AstNode::Try { value, wrap } => self.lower_try(builder, node, value, wrap),
            AstNode::Identifier(_) | AstNode::Call { .. }
                if matches!(self.type_in(builder, node), Ok(Ty::Record(record)) if self.constructor(builder, node, record).is_some()) =>
            {
                let Ok(Ty::Record(record)) = self.type_in(builder, node) else { unreachable!() };
                let (name, args) = self.constructor(builder, node, record).expect("a constructor");
                self.construct(builder, record, name, args).map(Some)
            }
            AstNode::Identifier(name) => {
                if builder.lookup(name).is_none()
                    && !self.global_names.contains_key(name)
                    && self.signatures.contains_key(name)
                {
                    return self.unsupported("a function used as a value");
                }
                let (place, _) = self.variable(builder, name)?;
                Ok(Some(match place {
                    Place::Local(id) => Operand::Local(id),
                    Place::Global(id) => Operand::Global(id),
                }))
            }
            AstNode::Index { target, index } => {
                let Ty::List(element) = self.type_in(builder, target)? else {
                    return self.unsupported("indexing a string (char)");
                };
                let list = self.expr(builder, target)?.expect("a list");
                let index = self.value_as(builder, index, &Ty::Int)?.expect("an int");
                let dest = builder.local(*element, None);
                builder.emit(Instruction::ListGet {
                    dest,
                    list,
                    index,
                    line: self.line,
                });
                Ok(Some(Operand::Local(dest)))
            }
            AstNode::Slice { target, start, end } => {
                let ty = self.type_in(builder, target)?;
                if !matches!(ty, Ty::List(_)) {
                    return self.unsupported("slicing a string");
                }
                let list = self.expr(builder, target)?.expect("a list");
                let start = match start {
                    Some(start) => Some(self.index(builder, start)?),
                    None => None,
                };
                let end = match end {
                    Some(end) => Some(self.index(builder, end)?),
                    None => None,
                };
                let dest = builder.local(ty, None);
                builder.emit(Instruction::ListSlice {
                    dest,
                    list,
                    start,
                    end,
                    line: self.line,
                });
                Ok(Some(Operand::Local(dest)))
            }
            AstNode::Unary { op, operand } => {
                let ty = self.type_in(builder, operand)?;
                let value = self.expr(builder, operand)?.expect("a value");
                let (op, result_type) = match (op, ty) {
                    (UnaryOperator::Negate, Ty::Int) => (UnaryOp::IntNeg, Ty::Int),
                    (UnaryOperator::Negate, Ty::Float) => (UnaryOp::FloatNeg, Ty::Float),
                    (UnaryOperator::Not, Ty::Bool) => (UnaryOp::Not, Ty::Bool),
                    (UnaryOperator::BitNot, Ty::Int) => (UnaryOp::BitNot, Ty::Int),
                    _ => return self.unsupported("this operator"),
                };
                Ok(Some(self.unary(builder, op, value, result_type)))
            }
            AstNode::BinaryOp { left, op, right } => match op {
                BinaryOperator::And | BinaryOperator::Or => {
                    self.short_circuit(builder, left, *op, right).map(Some)
                }
                BinaryOperator::In => self.contains(builder, left, right).map(Some),
                BinaryOperator::Is => {
                    let Ty::Record(record) = self.type_in(builder, left)? else {
                        return self.unsupported("'is' on this value");
                    };
                    let AstNode::Identifier(name) = right.unlocated() else {
                        return self.unsupported("'is' with this pattern");
                    };
                    let value = self.expr(builder, left)?.expect("a record");
                    self.is_variant(builder, value, record, name).map(Some)
                }
                _ => {
                    let mut left_type = self.type_in(builder, left)?;
                    let mut right_type = self.type_in(builder, right)?;
                    // `floats + [3]`: the literal gets the type of the other list.
                    if let (Ty::List(_), Ty::List(_)) = (&left_type, &right_type) {
                        if matches!(right.unlocated(), AstNode::ArrayLiteral(_)) {
                            right_type = left_type.clone();
                        } else if matches!(left.unlocated(), AstNode::ArrayLiteral(_)) {
                            left_type = right_type.clone();
                        }
                    }
                    let left_value = self.value_as(builder, left, &left_type)?.expect("a value");
                    let right_value = self.value_as(builder, right, &right_type)?.expect("a value");
                    let (result, _) = self.arithmetic(
                        builder,
                        *op,
                        left_value,
                        &left_type,
                        right_value,
                        &right_type,
                    )?;
                    Ok(Some(result))
                }
            },
            AstNode::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
            } => {
                let ty = self.type_in(builder, node)?;
                let dest = builder.local(ty.clone(), None);
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
                    let value = self.branch_value(builder, branch, &ty)?;
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
                if displays.borrow().iter().any(Option::is_some) {
                    return self.unsupported("a 'display' method");
                }
                match callee.unlocated() {
                    AstNode::Identifier(name) => {
                        if builder.lookup(name).is_some() || self.global_names.contains_key(name) {
                            return self.unsupported("calling a function stored in a variable");
                        }
                        self.call(builder, name, args)
                    }
                    AstNode::Member { object, name } => self.builtin_method(builder, node, object, name, args),
                    _ => self.unsupported("this call"),
                }
            }
            _ => self.unsupported(describe(node)),
        }
    }

    /// The value of an `if` branch used as an expression: the last expression of its block.
    fn branch_value(&mut self, builder: &mut Builder, branch: &AstNode, ty: &Ty) -> Lower<Option<Operand>> {
        match branch.unlocated() {
            AstNode::Block(statements) => {
                let Some((last, rest)) = statements.split_last() else {
                    return Ok(None);
                };
                builder.scopes.push(HashMap::new());
                let result = self
                    .statements(builder, rest)
                    .and_then(|_| self.value_as(builder, last, ty));
                builder.scopes.pop();
                result
            }
            other => self.value_as(builder, other, ty),
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

    /// `item in collection`, for a list or a string.
    fn contains(&mut self, builder: &mut Builder, item: &AstNode, collection: &AstNode) -> Lower<Operand> {
        match self.type_in(builder, collection)? {
            Ty::List(element) => {
                let item = self.value_as(builder, item, &element)?.expect("an item");
                let list = self.expr(builder, collection)?.expect("a list");
                Ok(self.binary(builder, BinaryOp::ListContains, item, list, Ty::Bool))
            }
            Ty::Str if self.type_in(builder, item)? == Ty::Str => {
                let needle = self.expr(builder, item)?.expect("a string");
                let haystack = self.expr(builder, collection)?.expect("a string");
                Ok(self.binary(builder, BinaryOp::StrContains, needle, haystack, Ty::Bool))
            }
            _ => self.unsupported("'in' with these types"),
        }
    }

    /// A binary operator on two values; returns the result and its type.
    fn arithmetic(
        &mut self,
        builder: &mut Builder,
        op: BinaryOperator,
        left: Operand,
        left_type: &Ty,
        right: Operand,
        right_type: &Ty,
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
        let numeric = |ty: &Ty| matches!(ty, Ty::Int | Ty::Float);
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
                let left = self.convert(builder, Some(left), l, &Ty::Float)?.expect("a value");
                let right = self.convert(builder, Some(right), r, &Ty::Float)?.expect("a value");
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
            (Ty::Record(_), Ty::Record(_)) => match comparison {
                Some(comparison @ (Comparison::Equal | Comparison::NotEqual)) => {
                    (BinaryOp::RecordCompare(comparison), left, right, Ty::Bool)
                }
                _ => return self.unsupported(&format!("'{}' on these values", op.symbol())),
            },
            (Ty::List(_), Ty::List(_)) => match (op, comparison) {
                (_, Some(comparison @ (Comparison::Equal | Comparison::NotEqual))) => {
                    (BinaryOp::ListCompare(comparison), left, right, Ty::Bool)
                }
                (B::Add, None) => (BinaryOp::ListConcat, left, right, left_type.clone()),
                _ => return self.unsupported(&format!("'{}' on lists", op.symbol())),
            },
            _ => return self.unsupported(&format!("'{}' on these types", op.symbol())),
        };
        let result = self.binary(builder, op, left, right, result_type.clone());
        Ok((result, result_type))
    }

    /// `items.push(x)`, `text.length()` and the other methods of lists and strings.
    fn builtin_method(
        &mut self,
        builder: &mut Builder,
        call: &AstNode,
        object: &AstNode,
        name: &str,
        args: &[AstNode],
    ) -> Lower<Option<Operand>> {
        let object_type = self.type_in(builder, object)?;
        let line = self.line;
        match (&object_type, name) {
            (Ty::Str, "length") => {
                let text = self.expr(builder, object)?.expect("a string");
                Ok(Some(self.unary(builder, UnaryOp::StrLength, text, Ty::Int)))
            }
            (Ty::List(_), "length") => {
                let list = self.expr(builder, object)?.expect("a list");
                Ok(Some(self.unary(builder, UnaryOp::ListLength, list, Ty::Int)))
            }
            (Ty::List(_), "join") => {
                let list = self.expr(builder, object)?.expect("a list");
                let separator = self.value_as(builder, &args[0], &Ty::Str)?.expect("a string");
                Ok(Some(self.binary(builder, BinaryOp::ListJoin, list, separator, Ty::Str)))
            }
            (Ty::List(element), "push") => {
                let list = self.target(builder, object)?;
                let value = self.value_as(builder, &args[0], element)?.expect("an item");
                builder.emit(Instruction::ListPush { list, value, line });
                Ok(None)
            }
            (Ty::List(element), "insert") => {
                let list = self.target(builder, object)?;
                let index = self.index(builder, &args[0])?;
                let value = self.value_as(builder, &args[1], element)?.expect("an item");
                builder.emit(Instruction::ListInsert {
                    list,
                    index,
                    value,
                    line,
                });
                Ok(None)
            }
            (Ty::List(_), "sort") => {
                let list = self.target(builder, object)?;
                builder.emit(Instruction::ListSort { list, line });
                Ok(None)
            }
            (Ty::Record(source), "mapErr") => {
                // Only with a variant constructor: `result.mapErr(Parse)`.
                let Ty::Record(target) = self.type_in(builder, call)? else {
                    return self.unsupported("'mapErr'");
                };
                let (source, target) = (*source, target);
                let err = self.variant_index(target, "Err")?;
                let Ty::Record(error_record) = self.records[target].variants[err].fields[0].1.clone() else {
                    return self.unsupported("'mapErr' with this error type");
                };
                let constructor = match args[0].unlocated() {
                    AstNode::Identifier(name) if self.constructor(builder, &args[0], error_record).is_some() => name.clone(),
                    _ => return self.unsupported("'mapErr' with a function (only a variant, as in 'mapErr(Parse)')"),
                };
                let value = self.expr(builder, object)?.expect("a Result");
                let subject = builder.local(Ty::Record(source), None);
                builder.emit(Instruction::Copy { dest: Place::Local(subject), value });
                let succeeded = self.is_variant(builder, Operand::Local(subject), source, "Ok")?;
                let dest = builder.local(Ty::Record(target), None);
                let (ok_block, err_block, exit) = (builder.new_block(), builder.new_block(), builder.new_block());
                builder.terminate(Terminator::Branch { condition: succeeded, then_block: ok_block, else_block: err_block });
                for (block, variant) in [(ok_block, "Ok"), (err_block, "Err")] {
                    builder.switch_to(block);
                    let field_type = self.records[source].variants[self.variant_index(source, variant)?].fields[0].1.clone();
                    let field = builder.local(field_type, None);
                    builder.emit(Instruction::FieldGet { dest: field, record: Operand::Local(subject), field: 0 });
                    let mut inner = Operand::Local(field);
                    if variant == "Err" {
                        let wrapped = builder.local(Ty::Record(error_record), None);
                        let index = self.variant_index(error_record, &constructor)?;
                        builder.emit(Instruction::RecordNew { dest: wrapped, variant: index, fields: vec![inner] });
                        inner = Operand::Local(wrapped);
                    }
                    let index = self.variant_index(target, variant)?;
                    builder.emit(Instruction::RecordNew { dest, variant: index, fields: vec![inner] });
                    builder.terminate(Terminator::Goto(exit));
                }
                builder.switch_to(exit);
                Ok(Some(Operand::Local(dest)))
            }
            (Ty::List(_), "pop" | "removeAt") => {
                let Ty::Record(option) = self.type_in(builder, call)? else {
                    return self.unsupported(&format!("'{}'", name));
                };
                let list = self.target(builder, object)?;
                let index = match args.first() {
                    Some(index) => Some(self.index(builder, index)?),
                    None => None,
                };
                let dest = builder.local(Ty::Record(option), None);
                builder.emit(Instruction::ListRemove { dest, list, index, line: self.line });
                Ok(Some(Operand::Local(dest)))
            }
            (Ty::List(element), "indexOf") => {
                let Ty::Record(option) = self.type_in(builder, call)? else {
                    return self.unsupported("'indexOf'");
                };
                let list = self.expr(builder, object)?.expect("a list");
                let value = self.value_as(builder, &args[0], element)?.expect("an item");
                let position = self.binary(builder, BinaryOp::ListIndexOf, list, value, Ty::Int);
                let found = self.binary(
                    builder,
                    BinaryOp::IntCompare(Comparison::GreaterEqual),
                    position.clone(),
                    Operand::Int(0),
                    Ty::Bool,
                );
                Ok(Some(self.option(builder, option, found, position)))
            }
            _ => self.unsupported(&format!("the method '{}'", name)),
        }
    }

    /// The position of the variant `name` in `record`.
    fn variant_index(&self, record: RecordId, name: &str) -> Lower<usize> {
        match self.records[record].variants.iter().position(|v| v.name.as_deref() == Some(name)) {
            Some(index) => Ok(index),
            None => self.unsupported(&format!("the variant '{}'", name)),
        }
    }

    /// Whether `value` (of type `record`) is the variant `name`.
    fn is_variant(&mut self, builder: &mut Builder, value: Operand, record: RecordId, name: &str) -> Lower<Operand> {
        let variant = self.variant_index(record, name)?;
        let tag = self.unary(builder, UnaryOp::RecordTag, value, Ty::Int);
        Ok(self.binary(
            builder,
            BinaryOp::IntCompare(Comparison::Equal),
            tag,
            Operand::Int(variant as i64),
            Ty::Bool,
        ))
    }

    /// `Some(value)` if `present`, else `None`, for the `Option` record `option`.
    fn option(&mut self, builder: &mut Builder, option: RecordId, present: Operand, value: Operand) -> Operand {
        let dest = builder.local(Ty::Record(option), None);
        let some = self.variant_index(option, "Some").expect("Option has Some");
        let none = self.variant_index(option, "None").expect("Option has None");
        let (then_block, else_block, exit) = (builder.new_block(), builder.new_block(), builder.new_block());
        builder.terminate(Terminator::Branch { condition: present, then_block, else_block });
        builder.switch_to(then_block);
        builder.emit(Instruction::RecordNew { dest, variant: some, fields: vec![value] });
        builder.terminate(Terminator::Goto(exit));
        builder.switch_to(else_block);
        builder.emit(Instruction::RecordNew { dest, variant: none, fields: Vec::new() });
        builder.terminate(Terminator::Goto(exit));
        builder.switch_to(exit);
        Operand::Local(dest)
    }

    /// `match`: the arms are tried in order; the first whose pattern matches (and whose guard
    /// holds) runs.
    fn lower_match(
        &mut self,
        builder: &mut Builder,
        node: &AstNode,
        scrutinee: &AstNode,
        arms: &[crate::parser::ast::MatchArm],
    ) -> Lower<Option<Operand>> {
        let result_type = self.type_in(builder, node).unwrap_or(Ty::Void);
        let result = (result_type != Ty::Void).then(|| builder.local(result_type.clone(), None));
        let scrutinee_type = self.type_in(builder, scrutinee)?;
        let value = self.expr(builder, scrutinee)?.expect("a value");
        // Kept in a local: the arms may change the variable that was matched.
        let subject = builder.local(scrutinee_type.clone(), None);
        builder.emit(Instruction::Copy { dest: Place::Local(subject), value });
        let exit = builder.new_block();
        for arm in arms {
            let next = builder.new_block();
            builder.scopes.push(HashMap::new());
            let arm_result = (|| -> Lower<()> {
                self.pattern(builder, &arm.pattern, Operand::Local(subject), &scrutinee_type, next)?;
                if let Some(guard) = &arm.guard {
                    let condition = self.condition(builder, guard)?;
                    let body = builder.new_block();
                    builder.terminate(Terminator::Branch { condition, then_block: body, else_block: next });
                    builder.switch_to(body);
                }
                match result {
                    Some(result) => {
                        if let Some(value) = self.branch_value(builder, &arm.body, &result_type)? {
                            builder.emit(Instruction::Copy { dest: Place::Local(result), value });
                        }
                    }
                    None => self.block(builder, &arm.body)?,
                }
                builder.terminate(Terminator::Goto(exit));
                Ok(())
            })();
            builder.scopes.pop();
            arm_result?;
            builder.switch_to(next);
        }
        // The checker makes sure that some arm matches.
        builder.terminate(Terminator::Unreachable);
        builder.switch_to(exit);
        Ok(result.map(Operand::Local))
    }

    /// Checks `pattern` against `value`, going to `fail` if it does not match, and declares the
    /// variables it binds.
    fn pattern(
        &mut self,
        builder: &mut Builder,
        pattern: &crate::parser::ast::Pattern,
        value: Operand,
        ty: &Ty,
        fail: BlockId,
    ) -> Lower<()> {
        use crate::parser::ast::Pattern;
        let test = |builder: &mut Builder, condition: Operand| {
            let ok = builder.new_block();
            builder.terminate(Terminator::Branch { condition, then_block: ok, else_block: fail });
            builder.switch_to(ok);
        };
        match pattern {
            Pattern::Wildcard => Ok(()),
            Pattern::Identifier(name) => {
                let is_variant = matches!(ty, Ty::Record(record)
                    if self.records[*record].variants.iter().any(|v| v.name.as_deref() == Some(name) && v.fields.is_empty()));
                if is_variant {
                    let Ty::Record(record) = ty else { unreachable!() };
                    let condition = self.is_variant(builder, value, *record, name)?;
                    test(builder, condition);
                } else {
                    let local = builder.local(ty.clone(), Some(name));
                    builder.emit(Instruction::Copy { dest: Place::Local(local), value });
                    builder.declare(name, Variable::Local(local));
                }
                Ok(())
            }
            Pattern::Variant { name, args } => {
                let Ty::Record(record) = ty else {
                    return self.unsupported("this pattern");
                };
                let record = *record;
                let condition = self.is_variant(builder, value.clone(), record, name)?;
                test(builder, condition);
                let variant = self.variant_index(record, name)?;
                let fields = self.records[record].variants[variant].fields.clone();
                for (field, (arg, (_, field_type))) in args.iter().zip(&fields).enumerate() {
                    if matches!(arg, Pattern::Wildcard) {
                        continue;
                    }
                    let dest = builder.local(field_type.clone(), None);
                    builder.emit(Instruction::FieldGet { dest, record: value.clone(), field });
                    self.pattern(builder, arg, Operand::Local(dest), field_type, fail)?;
                }
                Ok(())
            }
            Pattern::Literal(literal) => {
                let literal_type = self.type_in(builder, literal)?;
                let expected = self.expr(builder, literal)?.expect("a literal");
                let (condition, _) = self.arithmetic(builder, BinaryOperator::Equal, value, ty, expected, &literal_type)?;
                test(builder, condition);
                Ok(())
            }
        }
    }

    /// `value?`: the `Ok` or `Some` value; otherwise the function returns the `Err` (wrapped in
    /// the variant the checker chose) or `None`. At the top level, the program stops.
    fn lower_try(
        &mut self,
        builder: &mut Builder,
        node: &AstNode,
        value: &AstNode,
        wrap: &std::cell::RefCell<Option<crate::parser::ast::TryWrap>>,
    ) -> Lower<Option<Operand>> {
        let Ty::Record(record) = self.type_in(builder, value)? else {
            return self.unsupported("'?' on this value");
        };
        let is_result = self.records[record].variants.iter().any(|v| v.name.as_deref() == Some("Ok"));
        let success = if is_result { "Ok" } else { "Some" };
        let result_type = self.type_in(builder, node)?;
        let subject = self.expr(builder, value)?.expect("a value");
        let subject_local = builder.local(Ty::Record(record), None);
        builder.emit(Instruction::Copy { dest: Place::Local(subject_local), value: subject });
        let subject = Operand::Local(subject_local);
        let succeeded = self.is_variant(builder, subject.clone(), record, success)?;
        let (failure, done) = (builder.new_block(), builder.new_block());
        builder.terminate(Terminator::Branch { condition: succeeded, then_block: done, else_block: failure });

        builder.switch_to(failure);
        let error = if is_result {
            let error_type = self.records[record].variants[self.variant_index(record, "Err")?].fields[0].1.clone();
            let error = builder.local(error_type.clone(), None);
            builder.emit(Instruction::FieldGet { dest: error, record: subject.clone(), field: 0 });
            Some((Operand::Local(error), error_type))
        } else {
            None
        };
        if builder.is_main {
            let message = match error {
                Some((error, error_type)) => self.display(builder, error, &error_type)?,
                None => self.string("None"),
            };
            builder.emit(Instruction::Fail { message, line: self.line });
            builder.terminate(Terminator::Unreachable);
        } else {
            let Ty::Record(returned) = builder.ret.clone() else {
                return self.unsupported("'?' in this function");
            };
            let failure_value = match error {
                None => {
                    let dest = builder.local(Ty::Record(returned), None);
                    let none = self.variant_index(returned, "None")?;
                    builder.emit(Instruction::RecordNew { dest, variant: none, fields: Vec::new() });
                    Operand::Local(dest)
                }
                Some((error, _)) => {
                    let err = self.variant_index(returned, "Err")?;
                    let mut error = error;
                    if let Some(wrap) = &*wrap.borrow() {
                        if wrap.module.is_some() {
                            return self.unsupported("'?' that wraps an error of another module");
                        }
                        let Ty::Record(error_record) = self.records[returned].variants[err].fields[0].1.clone() else {
                            return self.unsupported("'?' with this error type");
                        };
                        let variant = self.variant_index(error_record, &wrap.variant)?;
                        let dest = builder.local(Ty::Record(error_record), None);
                        builder.emit(Instruction::RecordNew { dest, variant, fields: vec![error] });
                        error = Operand::Local(dest);
                    }
                    let dest = builder.local(Ty::Record(returned), None);
                    builder.emit(Instruction::RecordNew { dest, variant: err, fields: vec![error] });
                    Operand::Local(dest)
                }
            };
            builder.terminate(Terminator::Return(Some(failure_value)));
        }

        builder.switch_to(done);
        let dest = builder.local(result_type, None);
        builder.emit(Instruction::FieldGet { dest, record: subject, field: 0 });
        Ok(Some(Operand::Local(dest)))
    }

    fn call(&mut self, builder: &mut Builder, name: &str, args: &[AstNode]) -> Lower<Option<Operand>> {
        if let Some(signature) = self.signatures.get(name) {
            let (id, params, ret) = (signature.id, signature.params.clone(), signature.ret.clone());
            let mut values = Vec::new();
            for (arg, param) in args.iter().zip(&params) {
                values.push(self.value_as(builder, arg, param)?.expect("an argument"));
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
                    let ty = self.type_in(builder, arg)?;
                    let value = self.expr(builder, arg)?.expect("a value");
                    let text = self.display(builder, value, &ty)?;
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
                let ty = self.type_in(builder, &args[0])?;
                let value = self.expr(builder, &args[0])?.expect("a value");
                self.display(builder, value, &ty).map(Some)
            }
            "float" if args.len() == 1 && matches!(self.type_in(builder, &args[0])?, Ty::Int | Ty::Float) => {
                self.value_as(builder, &args[0], &Ty::Float)
            }
            _ => self.unsupported(&format!("'{}'", name)),
        }
    }
}

/// Whether a statement is an expression that gives a value (not a declaration, a loop or a
/// jump).
fn is_expression(node: &AstNode) -> bool {
    !matches!(
        node.unlocated(),
        AstNode::VarDecl { .. }
            | AstNode::Assign { .. }
            | AstNode::While { .. }
            | AstNode::For { .. }
            | AstNode::Return(_)
            | AstNode::Break
            | AstNode::Continue
            | AstNode::FunctionDecl { .. }
            | AstNode::TypeDecl { .. }
            | AstNode::Import { .. }
    )
}

/// What an unsupported expression is, for the error message.
fn describe(node: &AstNode) -> &'static str {
    match node {
        AstNode::MapLiteral(_) => "a map",
        AstNode::ObjectLiteral(_) => "an object",
        AstNode::Member { .. } => "a member access",
        AstNode::Match { .. } => "'match'",
        AstNode::Lambda { .. } => "an anonymous function",
        AstNode::Try { .. } => "'?'",
        AstNode::MutArg(_) => "a 'mut' argument",
        AstNode::If { .. } => "this 'if'",
        _ => "this expression",
    }
}
