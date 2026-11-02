//! The intermediate representation (IR) between the checked AST and the code generators.
//!
//! A program is a list of functions. Each function has typed local variables and a list of
//! basic blocks: straight-line instructions that end with a jump, a branch or a return. Every
//! operation is explicit and typed (`IntAdd`, `FloatAdd` and `StrConcat` are different
//! operations), so a code generator never needs the types of the source language.
//!
//! Memory: strings and lists are reference-counted, and copying one only adds a reference
//! (value semantics with copy-on-write). A local of type `Str` or `List` owns one reference.
//! Writing a local releases its previous value, and a function releases the values of its
//! locals when it returns. The code generator applies these rules; the IR does not spell out
//! the reference counting.

mod lower;
mod print;

pub use lower::lower_program;

pub type LocalId = usize;
pub type GlobalId = usize;
pub type BlockId = usize;
pub type FuncId = usize;
pub type StrId = usize;
pub type RecordId = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Str,
    List(Box<Ty>),
    /// A value of a declared type (`Program::records`).
    Record(RecordId),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Void,
}

impl Ty {
    /// Whether values of this type are reference-counted.
    pub fn is_managed(&self) -> bool {
        matches!(self, Ty::Str | Ty::List(_) | Ty::Record(_) | Ty::Map(..) | Ty::Set(_))
    }

    /// The type of the items of a list type.
    pub fn element(&self) -> &Ty {
        match self {
            Ty::List(element) => element,
            other => panic!("{:?} is not a list type", other),
        }
    }
}

/// A value read by an instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Local(LocalId),
    Global(GlobalId),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// A string literal of the program (`Program::strings`).
    Str(StrId),
}

/// Where an instruction writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    Local(LocalId),
    Global(GlobalId),
}

/// A value that an instruction changes in place: a variable, or a part of the value in a
/// variable (`people[0].tags` is `{ root: people, path: [Index(0), Field(tags)] }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub root: Place,
    pub path: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// An item of a list.
    Index(Operand),
    /// A field of a record (its position).
    Field(usize),
    /// The value of a key of a map.
    Key(Operand),
}

/// A declared type, with its type arguments applied (`Option[int]` and `Option[string]` are
/// two records).
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// How the type is written (`Option[int]`).
    pub name: String,
    /// The variants of a sum type, in order (the tag of a value is its position), or one
    /// variant without a name for an object type.
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: Option<String>,
    pub fields: Vec<(String, Ty)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    /// Panics on overflow (`-(-9223372036854775807 - 1)`).
    IntNeg,
    FloatNeg,
    Not,
    BitNot,
    IntToFloat,
    IntToStr,
    FloatToStr,
    BoolToStr,
    /// A list as written in code (`[1, 2]`).
    ListToStr,
    /// A record as written in code (`{ x: 1 }`, `Some(2)`).
    RecordToStr,
    /// The tag (variant) of a record, as an `int`.
    RecordTag,
    MapLength,
    /// A map or a set as written in code (`['a': 1]`, `Set([1])`).
    MapToStr,
    /// The keys of a map, or the items of a set, as a list.
    MapKeys,
    MapValues,
    SetFromList,
    StrLength,
    /// A string as written in code, in quotes (`'it\\'s'`).
    StrRepr,
    ListLength,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    // Integer arithmetic panics on overflow and division by zero.
    IntAdd,
    IntSub,
    IntMul,
    IntDiv,
    IntRem,
    IntPow,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    // Float arithmetic panics when the result is not finite, or on division by zero.
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatRem,
    FloatPow,
    IntCompare(Comparison),
    FloatCompare(Comparison),
    BoolCompare(Comparison),
    StrCompare(Comparison),
    StrConcat,
    /// `text * count`.
    StrRepeat,
    /// `needle in haystack`, with two strings.
    StrContains,
    ListConcat,
    /// `==` and `!=` on lists.
    ListCompare(Comparison),
    /// `item in list`: the left operand is the item.
    ListContains,
    /// `list.join(separator)`.
    ListJoin,
    /// `==` and `!=` on records.
    RecordCompare(Comparison),
    /// The position of `right` in the list `left`, or -1.
    ListIndexOf,
    /// `key in map` (also for sets): the left operand is the key.
    MapContains,
    /// `==` and `!=` on maps and sets.
    MapCompare(Comparison),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// `dest = value` (a string copy shares the text).
    Copy { dest: Place, value: Operand },
    Unary { dest: LocalId, op: UnaryOp, value: Operand, line: usize },
    Binary { dest: LocalId, op: BinaryOp, left: Operand, right: Operand, line: usize },
    /// A call. Each `(parameter, target)` of `writebacks` is a `mut` parameter (or `mut self`):
    /// when the function returns, the target gets the parameter's final value.
    Call {
        dest: Option<LocalId>,
        function: FuncId,
        args: Vec<Operand>,
        writebacks: Vec<(usize, Target)>,
        line: usize,
    },
    /// A new list with `items` (the type of `dest` gives the type of the items).
    ListNew { dest: LocalId, items: Vec<Operand> },
    /// `dest = list[index]` (panics when out of range).
    ListGet { dest: LocalId, list: Operand, index: Operand, line: usize },
    /// `list[index] = value`.
    ListSet { list: Target, index: Operand, value: Operand, line: usize },
    ListPush { list: Target, value: Operand, line: usize },
    ListInsert { list: Target, index: Operand, value: Operand, line: usize },
    ListSort { list: Target, line: usize },
    /// `dest = list[start..end]`; a missing bound is the start or the end of the list.
    ListSlice {
        dest: LocalId,
        list: Operand,
        start: Option<Operand>,
        end: Option<Operand>,
        line: usize,
    },
    /// A new record of the variant `variant` (the type of `dest` gives the record).
    RecordNew { dest: LocalId, variant: usize, fields: Vec<Operand> },
    /// `dest = record.field` (the field at position `field`).
    FieldGet { dest: LocalId, record: Operand, field: usize },
    /// `target.field = value`.
    FieldSet { record: Target, field: usize, value: Operand, line: usize },
    /// `dest = list.pop()` or `list.removeAt(index)`: `dest` is an `Option` record.
    ListRemove { dest: LocalId, list: Target, index: Option<Operand>, line: usize },
    /// A new map with `entries` (the type of `dest` gives the types).
    MapNew { dest: LocalId, entries: Vec<(Operand, Operand)> },
    /// `dest = map[key]` (panics when the key is missing).
    MapGet { dest: LocalId, map: Operand, key: Operand, line: usize },
    /// `dest = map.get(key)`: `dest` is an `Option` record.
    MapLookup { dest: LocalId, map: Operand, key: Operand },
    /// `map[key] = value`, or `set.add(key)` without a value.
    MapSet { map: Target, key: Operand, value: Option<Operand>, line: usize },
    /// `map.remove(key)` (`dest` is an `Option` record) or `set.remove(key)` (`dest` is a
    /// boolean).
    MapRemove { dest: LocalId, map: Target, key: Operand, line: usize },
    /// Ends the program because an `Err` or `None` reached the top level with `?`: prints
    /// `error: <location>: <message>` and exits with code 1.
    Fail { message: Operand, line: usize },
    /// Writes a string and a line break to the standard output.
    Print { value: Operand, line: usize },
    /// One step of `for i in range(...)`: when `next` has not passed `end` (in the direction
    /// of `step`) and `done` is false, `has_value` becomes true, `value` gets `next`, and
    /// `next` advances by `step` (an overflow sets `done` instead).
    RangeNext {
        has_value: LocalId,
        value: LocalId,
        next: LocalId,
        end: Operand,
        step: Operand,
        done: LocalId,
    },
    /// Panics if `step` is 0 (`range(0, 10, 0)`).
    RangeCheck { step: Operand, line: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Goto(BlockId),
    Branch { condition: Operand, then_block: BlockId, else_block: BlockId },
    Return(Option<Operand>),
    /// The end of a function that must return a value, which the checker guarantees is never
    /// reached.
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    pub ty: Ty,
    /// The variable's name in the source, if it has one (temporaries do not).
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// The first locals are the parameters, in order.
    pub params: usize,
    /// For each parameter, whether it is `mut`: its final value is given back to the caller.
    pub writeback: Vec<bool>,
    pub ret: Ty,
    pub locals: Vec<Local>,
    /// The first block is the entry.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub ty: Ty,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// The file name shown in panics (`main.kj:3: division by zero`).
    pub file: String,
    pub functions: Vec<Function>,
    /// The top-level statements of the main file, run when the program starts.
    pub main: FuncId,
    /// Top-level variables of the main file.
    pub globals: Vec<Global>,
    pub strings: Vec<String>,
    pub records: Vec<Record>,
}
