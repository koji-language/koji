//! The C code generator: turns the IR into one C file, which a C compiler links with the
//! runtime library (`runtime/`).
//!
//! Each IR function becomes a C function whose basic blocks are labels. Strings are pointers to
//! the runtime's reference-counted strings and lists: every such variable owns one reference
//! (`NULL` before its first assignment), and a function releases them when it returns.

use std::fmt::Write;

use crate::ir::*;

const PRELUDE: &str = include_str!("prelude.h");

/// The C source of `program`.
pub fn generate(program: &Program) -> String {
    let mut generator = Generator {
        program,
        out: String::new(),
    };
    generator.program();
    generator.out
}

struct Generator<'a> {
    program: &'a Program,
    out: String,
}

fn c_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int => "int64_t",
        Ty::Float => "double",
        Ty::Bool => "bool",
        Ty::Str => "KojiString *",
        Ty::List(_) => "KojiList *",
        Ty::Void => "void",
    }
}

/// `expression` (borrowed) with a new reference, for a managed type.
fn share(ty: &Ty, expression: String) -> String {
    match ty {
        Ty::Str => format!("koji_string_share({})", expression),
        Ty::List(_) => format!("koji_list_share({})", expression),
        _ => expression,
    }
}

fn release(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Str => Some("koji_string_release"),
        Ty::List(_) => Some("koji_list_release"),
        _ => None,
    }
}

/// The description of the items of a list (`koji_element_int`, ...).
fn element_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int => "&koji_element_int",
        Ty::Float => "&koji_element_float",
        Ty::Bool => "&koji_element_bool",
        Ty::Str => "&koji_element_string",
        Ty::List(_) => "&koji_element_list",
        Ty::Void => unreachable!("a list of void"),
    }
}

/// A C string literal with the bytes of `text`.
fn c_string(text: &str) -> String {
    let mut literal = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' | b'\\' | b'?' => {
                let _ = write!(literal, "\\{:03o}", byte);
            }
            0x20..=0x7e => literal.push(byte as char),
            _ => {
                let _ = write!(literal, "\\{:03o}", byte);
            }
        }
    }
    literal.push('"');
    literal
}

fn function_name(id: FuncId) -> String {
    format!("koji_f{}", id)
}

impl Generator<'_> {
    fn program(&mut self) {
        self.out.push_str(PRELUDE);
        self.out.push('\n');

        for (id, text) in self.program.strings.iter().enumerate() {
            let _ = writeln!(self.out, "static KojiString *koji_s{}; // {:?}", id, text);
        }
        for (id, global) in self.program.globals.iter().enumerate() {
            let _ = writeln!(self.out, "static {} koji_g{}; // {}", c_type(&global.ty), id, global.name);
        }
        self.out.push('\n');

        for (id, function) in self.program.functions.iter().enumerate() {
            let _ = writeln!(self.out, "{};", self.signature(id, function));
        }

        for (id, function) in self.program.functions.iter().enumerate() {
            self.out.push('\n');
            self.function(id, function);
        }

        self.out.push_str("\nint main(void) {\n");
        for (id, text) in self.program.strings.iter().enumerate() {
            let _ = writeln!(
                self.out,
                "    koji_s{} = koji_string_new({}, {});",
                id,
                c_string(text),
                text.len()
            );
        }
        let _ = writeln!(self.out, "    {}();", function_name(self.program.main));
        self.out.push_str("    return 0;\n}\n");
    }

    fn signature(&self, id: FuncId, function: &Function) -> String {
        let params: Vec<String> = (0..function.params)
            .map(|param| format!("{} l{}", c_type(&function.locals[param].ty), param))
            .collect();
        let params = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };
        format!(
            "static {} {}({}) /* {} */",
            c_type(&function.ret),
            function_name(id),
            params,
            function.name
        )
    }

    fn location(&self, line: usize) -> String {
        if line == 0 {
            c_string(&self.program.file)
        } else {
            c_string(&format!("{}:{}", self.program.file, line))
        }
    }

    fn function(&mut self, id: FuncId, function: &Function) {
        let _ = writeln!(self.out, "{} {{", self.signature(id, function));
        // Parameters are borrowed from the caller; a reference of their own keeps them alive
        // even if the caller's variable changes during the call.
        for (param, local) in function.locals.iter().enumerate().take(function.params) {
            match local.ty {
                Ty::Str => {
                    let _ = writeln!(self.out, "    koji_string_retain(l{});", param);
                }
                Ty::List(_) => {
                    let _ = writeln!(self.out, "    koji_list_retain(l{});", param);
                }
                _ => {}
            }
        }
        for (local_id, local) in function.locals.iter().enumerate().skip(function.params) {
            let initial = match local.ty {
                Ty::Str | Ty::List(_) => "NULL",
                Ty::Bool => "false",
                Ty::Float => "0.0",
                _ => "0",
            };
            let comment = local.name.as_ref().map(|name| format!(" // {}", name)).unwrap_or_default();
            let _ = writeln!(self.out, "    {} l{} = {};{}", c_type(&local.ty), local_id, initial, comment);
        }
        for (block_id, block) in function.blocks.iter().enumerate() {
            let _ = writeln!(self.out, "b{}:;", block_id);
            for instruction in &block.instructions {
                self.instruction(function, instruction);
            }
            self.terminator(function, &block.terminator);
        }
        self.out.push_str("}\n");
    }

    fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Local(id) => format!("l{}", id),
            Operand::Global(id) => format!("koji_g{}", id),
            Operand::Int(i64::MIN) => "INT64_MIN".to_string(),
            Operand::Int(n) => format!("INT64_C({})", n),
            Operand::Float(x) => format!("{:?}", x),
            Operand::Bool(b) => b.to_string(),
            Operand::Str(id) => format!("koji_s{}", id),
        }
    }

    fn place(&self, place: &Place) -> String {
        match place {
            Place::Local(id) => format!("l{}", id),
            Place::Global(id) => format!("koji_g{}", id),
        }
    }

    fn place_type<'a>(&'a self, function: &'a Function, place: &Place) -> &'a Ty {
        match place {
            Place::Local(id) => &function.locals[*id].ty,
            Place::Global(id) => &self.program.globals[*id].ty,
        }
    }

    /// `dest = expression`, where `expression` gives a new reference for a managed type.
    fn set(&mut self, dest: &str, ty: &Ty, expression: &str) {
        match ty {
            Ty::Str => {
                let _ = writeln!(self.out, "    koji_string_set(&{}, {});", dest, expression);
            }
            Ty::List(_) => {
                let _ = writeln!(self.out, "    koji_list_set_variable(&{}, {});", dest, expression);
            }
            _ => {
                let _ = writeln!(self.out, "    {} = {};", dest, expression);
            }
        }
    }

    /// The variable that holds the list of `target`, as a `KojiList **`.
    fn target(&self, target: &Target, line: usize) -> String {
        let mut variable = format!("&{}", self.place(&target.root));
        for index in &target.path {
            variable = format!(
                "(KojiList **)koji_list_slot({}, {}, {})",
                variable,
                self.operand(index),
                self.location(line)
            );
        }
        variable
    }

    /// Calls `call`, where `VALUE` is the address of `value` (the list makes its own copy).
    fn with_address(&mut self, ty: &Ty, value: &Operand, call: &str) {
        let _ = writeln!(
            self.out,
            "    {{ {} value = {}; {}; }}",
            c_type(ty),
            self.operand(value),
            call.replace("VALUE", "&value")
        );
    }

    fn instruction(&mut self, function: &Function, instruction: &Instruction) {
        match instruction {
            Instruction::Copy { dest, value } => {
                let ty = self.place_type(function, dest).clone();
                let expression = share(&ty, self.operand(value));
                let dest = self.place(dest);
                self.set(&dest, &ty, &expression);
            }
            Instruction::Unary { dest, op, value, line } => {
                let value = self.operand(value);
                let location = self.location(*line);
                let expression = match op {
                    UnaryOp::IntNeg => format!("koji_int_neg({}, {})", value, location),
                    UnaryOp::FloatNeg => format!("-({})", value),
                    UnaryOp::Not => format!("!({})", value),
                    UnaryOp::BitNot => format!("~({})", value),
                    UnaryOp::IntToFloat => format!("(double){}", value),
                    UnaryOp::IntToStr => format!("koji_string_from_int({})", value),
                    UnaryOp::FloatToStr => format!("koji_string_from_float({})", value),
                    UnaryOp::BoolToStr => format!("koji_string_from_bool({})", value),
                    UnaryOp::ListToStr => format!("koji_list_repr({})", value),
                    UnaryOp::StrLength => format!("koji_string_length({})", value),
                    UnaryOp::ListLength => format!("koji_list_length({})", value),
                };
                self.set(&format!("l{}", dest), &function.locals[*dest].ty, &expression);
            }
            Instruction::Binary { dest, op, left, right, line } => {
                let expression = match op {
                    // The item needs an address.
                    BinaryOp::ListContains => {
                        let item_type = self.operand_type(function, right).element().clone();
                        format!(
                            "({{ {} item = {}; koji_list_contains({}, &item); }})",
                            c_type(&item_type),
                            self.operand(left),
                            self.operand(right)
                        )
                    }
                    _ => self.binary(*op, &self.operand(left), &self.operand(right), *line),
                };
                self.set(&format!("l{}", dest), &function.locals[*dest].ty, &expression);
            }
            Instruction::Call { dest, function: callee, args, .. } => {
                let args: Vec<String> = args.iter().map(|arg| self.operand(arg)).collect();
                let call = format!("{}({})", function_name(*callee), args.join(", "));
                match dest {
                    Some(dest) => self.set(&format!("l{}", dest), &function.locals[*dest].ty, &call),
                    None => {
                        let _ = writeln!(self.out, "    {};", call);
                    }
                }
            }
            Instruction::ListNew { dest, items } => {
                let ty = function.locals[*dest].ty.clone();
                let element = ty.element().clone();
                let new = format!("koji_list_new({}, {})", element_type(&element), items.len());
                self.set(&format!("l{}", dest), &ty, &new);
                for item in items {
                    let call = format!("koji_list_push(&l{}, VALUE)", dest);
                    self.with_address(&element, item, &call);
                }
            }
            Instruction::ListGet { dest, list, index, line } => {
                let ty = function.locals[*dest].ty.clone();
                let item = format!(
                    "*({} const *)koji_list_get({}, {}, {})",
                    c_type(&ty),
                    self.operand(list),
                    self.operand(index),
                    self.location(*line)
                );
                self.set(&format!("l{}", dest), &ty, &share(&ty, item));
            }
            Instruction::ListSet { list, index, value, line } => {
                let ty = self.operand_type(function, value);
                let call = format!(
                    "koji_list_set({}, {}, VALUE, {})",
                    self.target(list, *line),
                    self.operand(index),
                    self.location(*line)
                );
                self.with_address(&ty, value, &call);
            }
            Instruction::ListPush { list, value, line } => {
                let ty = self.operand_type(function, value);
                let call = format!("koji_list_push({}, VALUE)", self.target(list, *line));
                self.with_address(&ty, value, &call);
            }
            Instruction::ListInsert { list, index, value, line } => {
                let ty = self.operand_type(function, value);
                let call = format!(
                    "koji_list_insert({}, {}, VALUE, {})",
                    self.target(list, *line),
                    self.operand(index),
                    self.location(*line)
                );
                self.with_address(&ty, value, &call);
            }
            Instruction::ListSort { list, line } => {
                let _ = writeln!(self.out, "    koji_list_sort({});", self.target(list, *line));
            }
            Instruction::ListSlice { dest, list, start, end, line } => {
                let bound = |bound: &Option<Operand>| match bound {
                    Some(bound) => format!("true, {}", self.operand(bound)),
                    None => "false, 0".to_string(),
                };
                let slice = format!(
                    "koji_list_slice({}, {}, {}, {})",
                    self.operand(list),
                    bound(start),
                    bound(end),
                    self.location(*line)
                );
                self.set(&format!("l{}", dest), &function.locals[*dest].ty, &slice);
            }
            Instruction::Print { value, line } => {
                let _ = writeln!(
                    self.out,
                    "    koji_print({}, {});",
                    self.operand(value),
                    self.location(*line)
                );
            }
            Instruction::RangeCheck { step, line } => {
                let _ = writeln!(
                    self.out,
                    "    if ({} == 0) koji_panic({}, \"range() step cannot be 0\");",
                    self.operand(step),
                    self.location(*line)
                );
            }
            Instruction::RangeNext { has_value, value, next, end, step, done } => {
                let _ = writeln!(
                    self.out,
                    "    l{} = koji_range_next(&l{}, {}, {}, &l{}, &l{});",
                    has_value,
                    next,
                    self.operand(end),
                    self.operand(step),
                    done,
                    value
                );
            }
        }
    }

    fn binary(&self, op: BinaryOp, left: &str, right: &str, line: usize) -> String {
        let location = self.location(line);
        let symbol = |comparison: Comparison| match comparison {
            Comparison::Equal => "==",
            Comparison::NotEqual => "!=",
            Comparison::Less => "<",
            Comparison::LessEqual => "<=",
            Comparison::Greater => ">",
            Comparison::GreaterEqual => ">=",
        };
        match op {
            BinaryOp::IntAdd => format!("koji_int_add({}, {}, {})", left, right, location),
            BinaryOp::IntSub => format!("koji_int_sub({}, {}, {})", left, right, location),
            BinaryOp::IntMul => format!("koji_int_mul({}, {}, {})", left, right, location),
            BinaryOp::IntDiv => format!("koji_int_div({}, {}, {})", left, right, location),
            BinaryOp::IntRem => format!("koji_int_rem({}, {}, {})", left, right, location),
            BinaryOp::IntPow => format!("koji_int_pow({}, {}, {})", left, right, location),
            BinaryOp::BitAnd => format!("({} & {})", left, right),
            BinaryOp::BitOr => format!("({} | {})", left, right),
            BinaryOp::BitXor => format!("({} ^ {})", left, right),
            BinaryOp::ShiftLeft => format!("koji_shift_left({}, {}, {})", left, right, location),
            BinaryOp::ShiftRight => format!("koji_shift_right({}, {}, {})", left, right, location),
            BinaryOp::FloatAdd => format!("koji_float_check({} + {}, {})", left, right, location),
            BinaryOp::FloatSub => format!("koji_float_check({} - {}, {})", left, right, location),
            BinaryOp::FloatMul => format!("koji_float_check({} * {}, {})", left, right, location),
            BinaryOp::FloatDiv => format!("koji_float_div({}, {}, {})", left, right, location),
            BinaryOp::FloatRem => format!("koji_float_rem({}, {}, {})", left, right, location),
            BinaryOp::FloatPow => format!("koji_float_pow({}, {}, {})", left, right, location),
            BinaryOp::IntCompare(comparison)
            | BinaryOp::FloatCompare(comparison)
            | BinaryOp::BoolCompare(comparison) => {
                format!("({} {} {})", left, symbol(comparison), right)
            }
            BinaryOp::StrCompare(Comparison::Equal) => format!("koji_string_equal({}, {})", left, right),
            BinaryOp::StrCompare(Comparison::NotEqual) => {
                format!("!koji_string_equal({}, {})", left, right)
            }
            BinaryOp::StrCompare(comparison) => {
                format!("(koji_string_compare({}, {}) {} 0)", left, right, symbol(comparison))
            }
            BinaryOp::StrConcat => format!("koji_string_concat({}, {})", left, right),
            BinaryOp::StrRepeat => format!("koji_string_repeat({}, {})", left, right),
            BinaryOp::StrContains => format!("koji_string_contains({}, {})", right, left),
            BinaryOp::ListConcat => format!("koji_list_concat({}, {})", left, right),
            BinaryOp::ListCompare(Comparison::NotEqual) => format!("!koji_list_equal({}, {})", left, right),
            BinaryOp::ListCompare(_) => format!("koji_list_equal({}, {})", left, right),
            BinaryOp::ListContains => unreachable!("generated by the caller"),
            BinaryOp::ListJoin => format!("koji_list_join({}, {})", left, right),
        }
    }

    /// Releases the strings and lists the function owns (before it returns).
    fn release_locals(&mut self, function: &Function) {
        for (id, local) in function.locals.iter().enumerate() {
            if let Some(release) = release(&local.ty) {
                let _ = writeln!(self.out, "    {}(l{});", release, id);
            }
        }
    }

    /// The type of an operand.
    fn operand_type(&self, function: &Function, operand: &Operand) -> Ty {
        match operand {
            Operand::Local(id) => function.locals[*id].ty.clone(),
            Operand::Global(id) => self.program.globals[*id].ty.clone(),
            Operand::Int(_) => Ty::Int,
            Operand::Float(_) => Ty::Float,
            Operand::Bool(_) => Ty::Bool,
            Operand::Str(_) => Ty::Str,
        }
    }

    fn terminator(&mut self, function: &Function, terminator: &Terminator) {
        match terminator {
            Terminator::Goto(block) => {
                let _ = writeln!(self.out, "    goto b{};", block);
            }
            Terminator::Branch { condition, then_block, else_block } => {
                let _ = writeln!(
                    self.out,
                    "    if ({}) goto b{}; else goto b{};",
                    self.operand(condition),
                    then_block,
                    else_block
                );
            }
            Terminator::Return(value) => {
                let value = value.as_ref().map(|value| self.operand(value));
                match value {
                    // The result is computed before the locals are released: it may be one of them.
                    Some(value) => {
                        let shared = share(&function.ret, value);
                        let _ = writeln!(self.out, "    {{ {} result = {};", c_type(&function.ret), shared);
                        self.release_locals(function);
                        self.out.push_str("    return result; }\n");
                    }
                    None => {
                        self.release_locals(function);
                        self.out.push_str("    return;\n");
                    }
                }
            }
            Terminator::Unreachable => {
                let _ = writeln!(
                    self.out,
                    "    koji_panic({}, \"the function ended without returning a value\");",
                    c_string(&self.program.file)
                );
            }
        }
    }
}
