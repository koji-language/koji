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
        Ty::Record(_) => "KojiRecord *",
        Ty::Map(..) | Ty::Set(_) => "KojiMap *",
        Ty::Void => "void",
    }
}

/// `expression` (borrowed) with a new reference, for a managed type.
fn share(ty: &Ty, expression: String) -> String {
    match ty {
        Ty::Str => format!("koji_string_share({})", expression),
        Ty::List(_) => format!("koji_list_share({})", expression),
        Ty::Record(_) => format!("koji_record_share({})", expression),
        Ty::Map(..) | Ty::Set(_) => format!("koji_map_share({})", expression),
        _ => expression,
    }
}

fn release(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Str => Some("koji_string_release"),
        Ty::List(_) => Some("koji_list_release"),
        Ty::Record(_) => Some("koji_record_release"),
        Ty::Map(..) | Ty::Set(_) => Some("koji_map_release"),
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
        Ty::Record(_) => "&koji_element_record",
        Ty::Map(..) | Ty::Set(_) => "&koji_element_map",
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
        self.records();
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

    /// The descriptions of the declared types, for the runtime.
    fn records(&mut self) {
        for (id, record) in self.program.records.iter().enumerate() {
            let _ = writeln!(self.out, "// {}", record.name);
            for (index, variant) in record.variants.iter().enumerate() {
                let names: Vec<String> = variant.fields.iter().map(|(name, _)| c_string(name)).collect();
                let types: Vec<&str> = variant.fields.iter().map(|(_, ty)| element_type(ty)).collect();
                if !variant.fields.is_empty() {
                    let _ = writeln!(
                        self.out,
                        "static const char *const koji_record_{}_{}_names[] = {{ {} }};",
                        id,
                        index,
                        names.join(", ")
                    );
                    let _ = writeln!(
                        self.out,
                        "static const KojiElementType *const koji_record_{}_{}_types[] = {{ {} }};",
                        id,
                        index,
                        types.join(", ")
                    );
                }
            }
            let variants: Vec<String> = record
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| {
                    let name = variant.name.as_deref().map(c_string).unwrap_or_else(|| "NULL".to_string());
                    if variant.fields.is_empty() {
                        format!("{{ {}, 0, NULL, NULL }}", name)
                    } else {
                        format!(
                            "{{ {}, {}, koji_record_{}_{}_names, koji_record_{}_{}_types }}",
                            name,
                            variant.fields.len(),
                            id,
                            index,
                            id,
                            index
                        )
                    }
                })
                .collect();
            let _ = writeln!(
                self.out,
                "static const KojiVariantType koji_record_{}_variants[] = {{ {} }};",
                id,
                variants.join(", ")
            );
            let _ = writeln!(
                self.out,
                "static const KojiRecordType koji_record_{} = {{ {}, koji_record_{}_variants }};",
                id,
                record.variants.len(),
                id
            );
        }
    }

    fn signature(&self, id: FuncId, function: &Function) -> String {
        let mut params: Vec<String> = (0..function.params)
            .map(|param| format!("{} l{}", c_type(&function.locals[param].ty), param))
            .collect();
        // A `mut` parameter gives its final value back through `out<n>`.
        for (param, writeback) in function.writeback.iter().enumerate() {
            if *writeback {
                params.push(format!("{} *out{}", c_type(&function.locals[param].ty), param));
            }
        }
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
            let retain = match local.ty {
                Ty::Str => "koji_string_retain",
                Ty::List(_) => "koji_list_retain",
                Ty::Record(_) => "koji_record_retain",
                _ => continue,
            };
            let _ = writeln!(self.out, "    {}(l{});", retain, param);
        }
        for (local_id, local) in function.locals.iter().enumerate().skip(function.params) {
            let initial = match local.ty {
                Ty::Str | Ty::List(_) | Ty::Record(_) => "NULL",
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
            Ty::Record(_) => {
                let _ = writeln!(self.out, "    koji_record_set_variable(&{}, {});", dest, expression);
            }
            Ty::Map(..) | Ty::Set(_) => {
                let _ = writeln!(self.out, "    koji_map_set_variable(&{}, {});", dest, expression);
            }
            _ => {
                let _ = writeln!(self.out, "    {} = {};", dest, expression);
            }
        }
    }

    /// The variable that holds the value of `target`, as a `void **` (each step copies a
    /// shared list or record before giving the slot inside it).
    fn path(&self, target: &Target, line: usize) -> String {
        let mut variable = format!("(void **)&{}", self.place(&target.root));
        for step in &target.path {
            variable = match step {
                Step::Index(index) => format!(
                    "(void **)koji_list_slot((KojiList **){}, {}, {})",
                    variable,
                    self.operand(index),
                    self.location(line)
                ),
                Step::Field(field) => format!("(void **)koji_record_slot((KojiRecord **){}, {})", variable, field),
                Step::Key(key) => format!(
                    "(void **)koji_map_slot((KojiMap **){}, {}, {})",
                    variable,
                    self.address(key),
                    self.location(line)
                ),
            };
        }
        variable
    }

    /// Writes `value` (a C variable that owns its value) to `target`, giving up its reference.
    fn store(&mut self, target: &Target, ty: &Ty, value: &str, line: usize) {
        let Some((last, steps)) = target.path.split_last() else {
            let root = self.place(&target.root);
            self.set(&root, ty, value);
            return;
        };
        let container = Target {
            root: target.root,
            path: steps.to_vec(),
        };
        let call = match last {
            Step::Index(index) => format!(
                "koji_list_set((KojiList **){}, {}, &{}, {})",
                self.path(&container, line),
                self.operand(index),
                value,
                self.location(line)
            ),
            Step::Field(field) => format!(
                "koji_record_set((KojiRecord **){}, {}, &{})",
                self.path(&container, line),
                field,
                value
            ),
            Step::Key(key) => format!(
                "koji_map_set((KojiMap **){}, {}, &{})",
                self.path(&container, line),
                self.address(key),
                value
            ),
        };
        let _ = writeln!(self.out, "    {};", call);
        if let Some(release) = release(ty) {
            let _ = writeln!(self.out, "    {}({});", release, value);
        }
    }

    /// The variable that holds the list of `target`, as a `KojiList **`.
    fn target(&self, target: &Target, line: usize) -> String {
        format!("(KojiList **){}", self.path(target, line))
    }

    /// The address of an operand that is a local or a global (a key kept in a local), or of a
    /// compound literal for a constant.
    fn address(&self, operand: &Operand) -> String {
        match operand {
            Operand::Local(_) | Operand::Global(_) => format!("&{}", self.operand(operand)),
            Operand::Int(_) => format!("&(int64_t){{ {} }}", self.operand(operand)),
            Operand::Float(_) => format!("&(double){{ {} }}", self.operand(operand)),
            Operand::Bool(_) => format!("&(bool){{ {} }}", self.operand(operand)),
            Operand::Str(_) => format!("&(KojiString *){{ {} }}", self.operand(operand)),
        }
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
                    UnaryOp::StrRepr => format!("koji_string_repr({})", value),
                    UnaryOp::ListLength => format!("koji_list_length({})", value),
                    UnaryOp::RecordToStr => format!("koji_record_repr({})", value),
                    UnaryOp::RecordTag => format!("koji_record_tag({})", value),
                    UnaryOp::MapLength => format!("koji_map_length({})", value),
                    UnaryOp::MapToStr => format!("koji_map_repr({})", value),
                    UnaryOp::MapKeys => format!("koji_map_keys({})", value),
                    UnaryOp::MapValues => format!("koji_map_values({})", value),
                    UnaryOp::SetFromList => format!("koji_set_from_list({})", value),
                };
                self.set(&format!("l{}", dest), &function.locals[*dest].ty, &expression);
            }
            Instruction::Binary { dest, op, left, right, line } => {
                let expression = match op {
                    BinaryOp::MapContains => format!("koji_map_contains({}, {})", self.operand(right), self.address(left)),
                    BinaryOp::ListIndexOf => {
                        let item_type = self.operand_type(function, left).element().clone();
                        format!(
                            "({{ {} item = {}; koji_list_index_of({}, &item); }})",
                            c_type(&item_type),
                            self.operand(right),
                            self.operand(left)
                        )
                    }
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
            Instruction::Call { dest, function: callee, args, writebacks, line } => {
                let mut values: Vec<String> = args.iter().map(|arg| self.operand(arg)).collect();
                // The final values of the `mut` parameters, owned by the caller after the call
                // (in a C block: the names are reused by the next call).
                let mut given_back = Vec::new();
                if !writebacks.is_empty() {
                    self.out.push_str("    {\n");
                }
                for (param, target) in writebacks {
                    let ty = self.operand_type(function, &args[*param]);
                    let name = format!("back{}", param);
                    let initial = if ty.is_managed() { "NULL" } else { "0" };
                    let _ = writeln!(self.out, "    {} {} = {};", c_type(&ty), name, initial);
                    values.push(format!("&{}", name));
                    given_back.push((name, ty, target));
                }
                let call = format!("{}({})", function_name(*callee), values.join(", "));
                match dest {
                    Some(dest) => self.set(&format!("l{}", dest), &function.locals[*dest].ty, &call),
                    None => {
                        let _ = writeln!(self.out, "    {};", call);
                    }
                }
                for (name, ty, target) in &given_back {
                    self.store(target, ty, name, *line);
                }
                if !given_back.is_empty() {
                    self.out.push_str("    }\n");
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
            Instruction::RecordNew { dest, variant, fields } => {
                let ty = function.locals[*dest].ty.clone();
                let Ty::Record(record) = ty else { unreachable!("a record") };
                let new = format!("koji_record_new(&koji_record_{}, {})", record, variant);
                self.set(&format!("l{}", dest), &ty, &new);
                let field_types: Vec<Ty> = self.program.records[record].variants[*variant]
                    .fields
                    .iter()
                    .map(|(_, ty)| ty.clone())
                    .collect();
                for (index, (field, field_type)) in fields.iter().zip(&field_types).enumerate() {
                    let call = format!("koji_record_init(l{}, {}, VALUE)", dest, index);
                    self.with_address(field_type, field, &call);
                }
            }
            Instruction::FieldGet { dest, record, field } => {
                let ty = function.locals[*dest].ty.clone();
                let value = format!("*({} const *)koji_record_get({}, {})", c_type(&ty), self.operand(record), field);
                self.set(&format!("l{}", dest), &ty, &share(&ty, value));
            }
            Instruction::FieldSet { record, field, value, line } => {
                let ty = self.operand_type(function, value);
                let call = format!(
                    "koji_record_set((KojiRecord **){}, {}, VALUE)",
                    self.path(record, *line),
                    field
                );
                self.with_address(&ty, value, &call);
            }
            Instruction::ListRemove { dest, list, index, line } => {
                let ty = function.locals[*dest].ty.clone();
                let Ty::Record(option) = ty else { unreachable!("an Option") };
                let variants = &self.program.records[option].variants;
                let position = |name: &str| variants.iter().position(|v| v.name.as_deref() == Some(name)).expect("Option");
                let (some, none) = (position("Some"), position("None"));
                let item_type = variants[some].fields[0].1.clone();
                let remove = match index {
                    Some(index) => format!("koji_list_remove({}, {}, &item)", self.target(list, *line), self.operand(index)),
                    None => format!("koji_list_pop({}, &item)", self.target(list, *line)),
                };
                // The list gives its reference to the item; the record takes its own.
                let release_item = match release(&item_type) {
                    Some(release) => format!(" {}(item);", release),
                    None => String::new(),
                };
                let _ = writeln!(
                    self.out,
                    "    {{ {} item; KojiRecord *option; if ({}) {{ option = koji_record_new(&koji_record_{}, {}); koji_record_init(option, 0, &item);{} }} else {{ option = koji_record_new(&koji_record_{}, {}); }} koji_record_set_variable(&l{}, option); }}",
                    c_type(&item_type),
                    remove,
                    option,
                    some,
                    release_item,
                    option,
                    none,
                    dest
                );
            }
            Instruction::MapNew { dest, entries } => {
                let ty = function.locals[*dest].ty.clone();
                let Ty::Map(key_type, value_type) = &ty else { unreachable!("a map") };
                let new = format!("koji_map_new({}, {})", element_type(key_type), element_type(value_type));
                self.set(&format!("l{}", dest), &ty, &new);
                for (key, value) in entries {
                    let _ = writeln!(
                        self.out,
                        "    {{ {} key = {}; {} value = {}; koji_map_set(&l{}, &key, &value); }}",
                        c_type(key_type),
                        self.operand(key),
                        c_type(value_type),
                        self.operand(value),
                        dest
                    );
                }
            }
            Instruction::MapGet { dest, map, key, line } => {
                let ty = function.locals[*dest].ty.clone();
                let value = format!(
                    "*({} const *)koji_map_get({}, {}, {})",
                    c_type(&ty),
                    self.operand(map),
                    self.address(key),
                    self.location(*line)
                );
                self.set(&format!("l{}", dest), &ty, &share(&ty, value));
            }
            Instruction::MapLookup { dest, map, key } => {
                let Ty::Record(option) = function.locals[*dest].ty else { unreachable!("an Option") };
                let variants = &self.program.records[option].variants;
                let position = |name: &str| variants.iter().position(|v| v.name.as_deref() == Some(name)).expect("Option");
                let (some, none) = (position("Some"), position("None"));
                let _ = writeln!(
                    self.out,
                    "    {{ const void *found = koji_map_find({}, {}); KojiRecord *option = koji_record_new(&koji_record_{}, found ? {} : {}); if (found) koji_record_init(option, 0, found); koji_record_set_variable(&l{}, option); }}",
                    self.operand(map),
                    self.address(key),
                    option,
                    some,
                    none,
                    dest
                );
            }
            Instruction::MapSet { map, key, value, line } => {
                let map = format!("(KojiMap **){}", self.path(map, *line));
                match value {
                    Some(value) => {
                        let ty = self.operand_type(function, value);
                        let call = format!("koji_map_set({}, {}, VALUE)", map, self.address(key));
                        self.with_address(&ty, value, &call);
                    }
                    None => {
                        let _ = writeln!(self.out, "    koji_map_set({}, {}, NULL);", map, self.address(key));
                    }
                }
            }
            Instruction::MapRemove { dest, map, key, line } => {
                let map = format!("(KojiMap **){}", self.path(map, *line));
                match function.locals[*dest].ty {
                    Ty::Record(option) => {
                        let variants = &self.program.records[option].variants;
                        let position = |name: &str| variants.iter().position(|v| v.name.as_deref() == Some(name)).expect("Option");
                        let (some, none) = (position("Some"), position("None"));
                        let value_type = variants[some].fields[0].1.clone();
                        let release_value = match release(&value_type) {
                            Some(release) => format!(" {}(value);", release),
                            None => String::new(),
                        };
                        let _ = writeln!(
                            self.out,
                            "    {{ {} value; KojiRecord *option; if (koji_map_remove({}, {}, &value)) {{ option = koji_record_new(&koji_record_{}, {}); koji_record_init(option, 0, &value);{} }} else {{ option = koji_record_new(&koji_record_{}, {}); }} koji_record_set_variable(&l{}, option); }}",
                            c_type(&value_type),
                            map,
                            self.address(key),
                            option,
                            some,
                            release_value,
                            option,
                            none,
                            dest
                        );
                    }
                    _ => {
                        let _ = writeln!(self.out, "    l{} = koji_map_remove({}, {}, NULL);", dest, map, self.address(key));
                    }
                }
            }
            Instruction::Fail { message, line } => {
                let _ = writeln!(self.out, "    koji_fail({}, {});", self.location(*line), self.operand(message));
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
            BinaryOp::ListContains | BinaryOp::ListIndexOf => unreachable!("generated by the caller"),
            BinaryOp::MapContains => unreachable!("generated by the caller"),
            BinaryOp::MapCompare(Comparison::NotEqual) => format!("!koji_map_equal({}, {})", left, right),
            BinaryOp::MapCompare(_) => format!("koji_map_equal({}, {})", left, right),
            BinaryOp::RecordCompare(Comparison::NotEqual) => format!("!koji_record_equal({}, {})", left, right),
            BinaryOp::RecordCompare(_) => format!("koji_record_equal({}, {})", left, right),
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
                // The `mut` parameters give their final values back.
                for (param, writeback) in function.writeback.iter().enumerate() {
                    if *writeback {
                        let ty = &function.locals[param].ty;
                        let _ = writeln!(self.out, "    *out{} = {};", param, share(ty, format!("l{}", param)));
                    }
                }
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
