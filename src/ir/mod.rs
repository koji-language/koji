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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Str,
    List(Box<Ty>),
    Void,
}

impl Ty {
    /// Whether values of this type are reference-counted.
    pub fn is_managed(&self) -> bool {
        matches!(self, Ty::Str | Ty::List(_))
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

/// A list that an instruction changes: a variable, or an item of a list in a variable
/// (`grid[0]` is `{ root: grid, path: [0] }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub root: Place,
    pub path: Vec<Operand>,
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
    StrLength,
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
    Call { dest: Option<LocalId>, function: FuncId, args: Vec<Operand>, line: usize },
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
}
