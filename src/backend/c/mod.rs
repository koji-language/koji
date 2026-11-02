//! The C code generator: turns the IR into one C file, which a C compiler links with the
//! runtime library (`runtime/`).
//!
//! Each IR function becomes a C function whose basic blocks are labels. Strings are pointers to
//! the runtime's reference-counted strings: every string variable owns one reference (`NULL`
//! before its first assignment), and a function releases them when it returns.

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

fn c_type(ty: Ty) -> &'static str {
    match ty {
        Ty::Int => "int64_t",
        Ty::Float => "double",
        Ty::Bool => "bool",
        Ty::Str => "KojiString *",
        Ty::Void => "void",
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
            let _ = writeln!(self.out, "static {} koji_g{}; // {}", c_type(global.ty), id, global.name);
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
            .map(|param| format!("{} l{}", c_type(function.locals[param].ty), param))
            .collect();
        let params = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };
        format!(
            "static {} {}({}) /* {} */",
            c_type(function.ret),
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
            if local.ty == Ty::Str {
                let _ = writeln!(self.out, "    koji_string_retain(l{});", param);
            }
        }
        for (local_id, local) in function.locals.iter().enumerate().skip(function.params) {
            let initial = match local.ty {
                Ty::Str => "NULL",
                Ty::Bool => "false",
                Ty::Float => "0.0",
                _ => "0",
            };
            let comment = local.name.as_ref().map(|name| format!(" // {}", name)).unwrap_or_default();
            let _ = writeln!(self.out, "    {} l{} = {};{}", c_type(local.ty), local_id, initial, comment);
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

    fn place_type(&self, function: &Function, place: &Place) -> Ty {
        match place {
            Place::Local(id) => function.locals[*id].ty,
            Place::Global(id) => self.program.globals[*id].ty,
        }
    }

    /// `dest = expression`, where `expression` gives a new string reference when `ty` is `Str`.
    fn set(&mut self, dest: &str, ty: Ty, expression: &str) {
        if ty == Ty::Str {
            let _ = writeln!(self.out, "    koji_string_set(&{}, {});", dest, expression);
        } else {
            let _ = writeln!(self.out, "    {} = {};", dest, expression);
        }
    }

    fn instruction(&mut self, function: &Function, instruction: &Instruction) {
        match instruction {
            Instruction::Copy { dest, value } => {
                let ty = self.place_type(function, dest);
                let value = self.operand(value);
                let expression = if ty == Ty::Str {
                    format!("koji_string_share({})", value)
                } else {
                    value
                };
                let dest = self.place(dest);
                self.set(&dest, ty, &expression);
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
                };
                self.set(&format!("l{}", dest), function.locals[*dest].ty, &expression);
            }
            Instruction::Binary { dest, op, left, right, line } => {
                let expression = self.binary(*op, &self.operand(left), &self.operand(right), *line);
                self.set(&format!("l{}", dest), function.locals[*dest].ty, &expression);
            }
            Instruction::Call { dest, function: callee, args, .. } => {
                let args: Vec<String> = args.iter().map(|arg| self.operand(arg)).collect();
                let call = format!("{}({})", function_name(*callee), args.join(", "));
                match dest {
                    Some(dest) => self.set(&format!("l{}", dest), function.locals[*dest].ty, &call),
                    None => {
                        let _ = writeln!(self.out, "    {};", call);
                    }
                }
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
        }
    }

    /// Releases the strings the function owns (before it returns).
    fn release_locals(&mut self, function: &Function) {
        for (id, local) in function.locals.iter().enumerate() {
            if local.ty == Ty::Str {
                let _ = writeln!(self.out, "    koji_string_release(l{});", id);
            }
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
                        let shared = if function.ret == Ty::Str {
                            format!("koji_string_share({})", value)
                        } else {
                            value
                        };
                        let _ = writeln!(self.out, "    {{ {} result = {};", c_type(function.ret), shared);
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
