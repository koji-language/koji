//! A readable text form of the IR (`kojic build --emit ir`), for debugging.

use std::fmt;

use super::*;

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (id, text) in self.strings.iter().enumerate() {
            writeln!(f, "string s{} = {:?}", id, text)?;
        }
        for (id, record) in self.records.iter().enumerate() {
            writeln!(f, "record r{} {}", id, record.name)?;
            for variant in &record.variants {
                let fields: Vec<String> = variant.fields.iter().map(|(name, ty)| format!("{}: {:?}", name, ty)).collect();
                writeln!(f, "    {} ({})", variant.name.as_deref().unwrap_or("{}"), fields.join(", "))?;
            }
        }
        for (id, global) in self.globals.iter().enumerate() {
            writeln!(f, "global g{}: {:?} // {}", id, global.ty, global.name)?;
        }
        for (id, function) in self.functions.iter().enumerate() {
            writeln!(f)?;
            write!(f, "function f{} {}(", id, function.name)?;
            for param in 0..function.params {
                if param > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "l{}: {:?}", param, function.locals[param].ty)?;
            }
            writeln!(f, "): {:?}", function.ret)?;
            for (id, local) in function.locals.iter().enumerate().skip(function.params) {
                match &local.name {
                    Some(name) => writeln!(f, "    local l{}: {:?} // {}", id, local.ty, name)?,
                    None => writeln!(f, "    local l{}: {:?}", id, local.ty)?,
                }
            }
            for (id, block) in function.blocks.iter().enumerate() {
                writeln!(f, "  b{}:", id)?;
                for instruction in &block.instructions {
                    writeln!(f, "    {}", instruction)?;
                }
                writeln!(f, "    {}", block.terminator)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Local(id) => write!(f, "l{}", id),
            Operand::Global(id) => write!(f, "g{}", id),
            Operand::Int(n) => write!(f, "{}", n),
            Operand::Float(x) => write!(f, "{:?}", x),
            Operand::Bool(b) => write!(f, "{}", b),
            Operand::Str(id) => write!(f, "s{}", id),
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Local(id) => write!(f, "l{}", id),
            Place::Global(id) => write!(f, "g{}", id),
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Index(index) => write!(f, "{}", index),
            Step::Field(field) => write!(f, ".{}", field),
            Step::Key(key) => write!(f, "{{{}}}", key),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root)?;
        for index in &self.path {
            write!(f, "[{}]", index)?;
        }
        Ok(())
    }
}

fn list(operands: &[Operand]) -> String {
    operands.iter().map(Operand::to_string).collect::<Vec<_>>().join(", ")
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::Copy { dest, value } => write!(f, "{} = {}", dest, value),
            Instruction::Unary { dest, op, value, .. } => write!(f, "l{} = {:?} {}", dest, op, value),
            Instruction::Binary { dest, op, left, right, .. } => {
                write!(f, "l{} = {:?} {}, {}", dest, op, left, right)
            }
            Instruction::Call { writebacks, .. } if !writebacks.is_empty() => {
                let Instruction::Call { dest, function, args, .. } = self else { unreachable!() };
                let dest = dest.map(|dest| format!("l{} = ", dest)).unwrap_or_default();
                let targets: Vec<String> = writebacks.iter().map(|(param, target)| format!("{} -> {}", param, target)).collect();
                write!(f, "{}call f{}({}) writeback {}", dest, function, list(args), targets.join(", "))
            }
            Instruction::Call { dest: Some(dest), function, args, .. } => {
                write!(f, "l{} = call f{}({})", dest, function, list(args))
            }
            Instruction::Call { dest: None, function, args, .. } => {
                write!(f, "call f{}({})", function, list(args))
            }
            Instruction::Print { value, .. } => write!(f, "print {}", value),
            Instruction::ListNew { dest, items } => write!(f, "l{} = list [{}]", dest, list(items)),
            Instruction::ListGet { dest, list, index, .. } => write!(f, "l{} = {}[{}]", dest, list, index),
            Instruction::ListSet { list, index, value, .. } => write!(f, "{}[{}] = {}", list, index, value),
            Instruction::ListPush { list, value, .. } => write!(f, "push {}, {}", list, value),
            Instruction::ListInsert { list, index, value, .. } => {
                write!(f, "insert {}, {}, {}", list, index, value)
            }
            Instruction::ListSort { list, .. } => write!(f, "sort {}", list),
            Instruction::RecordNew { dest, variant, fields } => {
                write!(f, "l{} = record variant {} ({})", dest, variant, list(fields))
            }
            Instruction::FieldGet { dest, record, field } => write!(f, "l{} = {}.{}", dest, record, field),
            Instruction::FieldSet { record, field, value, .. } => write!(f, "{}.{} = {}", record, field, value),
            Instruction::ListRemove { dest, list, index: Some(index), .. } => {
                write!(f, "l{} = remove {}, {}", dest, list, index)
            }
            Instruction::ListRemove { dest, list, index: None, .. } => write!(f, "l{} = pop {}", dest, list),
            Instruction::Fail { message, .. } => write!(f, "fail {}", message),
            Instruction::MapNew { dest, entries } => {
                let entries: Vec<String> = entries.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                write!(f, "l{} = map [{}]", dest, entries.join(", "))
            }
            Instruction::MapGet { dest, map, key, .. } => write!(f, "l{} = {}{{{}}}", dest, map, key),
            Instruction::MapLookup { dest, map, key } => write!(f, "l{} = lookup {}, {}", dest, map, key),
            Instruction::MapSet { map, key, value: Some(value), .. } => write!(f, "{}{{{}}} = {}", map, key, value),
            Instruction::MapSet { map, key, value: None, .. } => write!(f, "add {}, {}", map, key),
            Instruction::MapRemove { dest, map, key, .. } => write!(f, "l{} = remove {}, {}", dest, map, key),
            Instruction::ListSlice { dest, list, start, end, .. } => {
                let bound = |bound: &Option<Operand>| bound.as_ref().map(Operand::to_string).unwrap_or_default();
                write!(f, "l{} = {}[{}..{}]", dest, list, bound(start), bound(end))
            }
            Instruction::RangeNext { has_value, value, next, end, step, done } => write!(
                f,
                "l{}, l{} = range_next l{}, {}, {}, l{}",
                has_value, value, next, end, step, done
            ),
            Instruction::RangeCheck { step, .. } => write!(f, "range_check {}", step),
        }
    }
}

impl fmt::Display for Terminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminator::Goto(block) => write!(f, "goto b{}", block),
            Terminator::Branch { condition, then_block, else_block } => {
                write!(f, "branch {}, b{}, b{}", condition, then_block, else_block)
            }
            Terminator::Return(Some(value)) => write!(f, "return {}", value),
            Terminator::Return(None) => write!(f, "return"),
            Terminator::Unreachable => write!(f, "unreachable"),
        }
    }
}
