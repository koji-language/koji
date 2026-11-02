//! The type checker: checks every module of a program before it runs.

use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::EzError;
use crate::parser::ast::{
    AssignOperator, AstNode, BinaryOperator, Field, ImportKind, MatchArm, MethodRef, Param, ParamMode, Pattern,
    TypeParam,
    TemplatePart, TryWrap, TypeDef, TypeExpr, UnaryOperator,
};
use crate::parser::parse_source;
use crate::semantic::builtin_sums::{stdlib_functions, BUILTIN_SUMS};
use crate::semantic::types::{Constraint, ModuleId, Type, TypeId, TypeKind, TypeTable, VariantDef};

mod modules;
mod stdlib;
mod typing;
mod declarations;
mod statements;
mod functions;
mod generics;
use generics::GenericCallee;
mod expressions;
mod calls;
mod collections;
mod ownership;
mod patterns;

type CheckResult<T> = Result<T, EzError>;

/// Variables whose value was moved, by scope index and name, with the line of the move.
type Moves = HashMap<(usize, String), usize>;

/// Names of the standard library modules (`import math`).
pub const STDLIB_MODULES: &[&str] = &["time", "file", "path", "datetime", "math", "http", "json", "regex", "system"];

/// A checked program: the parsed modules, ready to be executed.
#[derive(Debug, Clone)]
pub struct Program {
    pub entry: PathBuf,
    pub modules: HashMap<PathBuf, Rc<Vec<AstNode>>>,
    pub ownership: Ownership,
    /// The declared types (used by the compiler).
    pub table: Rc<TypeTable>,
}

/// What the interpreter needs to know about move-only values, found by the type checker. AST
/// nodes are identified by their address (`node_id`): the parsed modules are never moved.
#[derive(Debug, Clone, Default)]
pub struct Ownership {
    /// Variables read by moving them out (`g = f` reads `f`).
    pub moves: HashSet<usize>,
    /// Declarations of a variable that owns a move-only value: it is dropped at the end of its
    /// block.
    pub declares: HashSet<usize>,
    /// Assignments that replace a move-only value: the old one is dropped.
    pub replaces: HashSet<usize>,
    /// Move-only values that are only used in place (`file.open(...)?` as a statement, or as an
    /// argument): they are dropped after that use.
    pub temporaries: HashSet<usize>,
    /// Scrutinees of a `match` on a move-only temporary: its bindings own their values.
    pub owning_matches: HashSet<usize>,
    /// Object literals of a type with a `drop` method, with that method (module, `Type.drop`).
    pub drops: HashMap<usize, (PathBuf, String)>,
    /// String literals that are `char` values (`c: char = 'a'`).
    pub char_literals: HashSet<usize>,
    /// Calls of built-in methods that change the value they are called on (`items.push(x)`),
    /// by their callee (`items.push`).
    pub mutating_calls: HashSet<usize>,
    /// The type of each expression, as checked (used by the compiler).
    pub types: HashMap<usize, Type>,
    /// The type of each function declaration (with `self` first for a method; a generic one
    /// uses `Type::Param` for its type parameters, the owner's first).
    pub function_types: HashMap<usize, Type>,
    /// The type arguments of each call of a generic function or method, as inferred
    /// (`first([1, 2])` calls `first[int]`). Inside a generic function they can contain its
    /// own type parameters.
    pub type_args: HashMap<usize, Vec<Type>>,
    /// The fields of each object literal, in the order of its type's declaration.
    pub field_orders: HashMap<usize, Vec<String>>,
    /// `int` expressions stored as `float` (the items of `[1, 2.5]`).
    pub to_float: HashSet<usize>,
    /// The type of each declared variable (`VarDecl`), after the later uses that refine it
    /// (`mut items = []` then `items.push(1)`).
    pub variable_types: HashMap<usize, Type>,
}

/// Identity of an AST node, for the `Ownership` tables.
pub fn node_id(node: &AstNode) -> usize {
    node.unlocated() as *const AstNode as usize
}

/// Resolves the path of a user module relative to the directory of the importing file.
pub fn resolve_module_path(dir: &Path, path: &str) -> PathBuf {
    let joined = dir.join(path);
    joined.canonicalize().unwrap_or(joined)
}

pub fn is_stdlib_import(path: &str) -> bool {
    STDLIB_MODULES.contains(&path)
}

#[derive(Debug, Clone)]
enum SymbolKind {
    Value,
    /// Built-in function, identified by its qualified name (`print`, `math.sqrt`).
    Builtin(String),
    /// Variant constructor of a sum type.
    Constructor { id: TypeId, variant: String },
}

#[derive(Debug, Clone)]
struct Symbol {
    ty: Type,
    /// The variable cannot be reassigned (`immut`, functions, parameters...).
    is_const: bool,
    /// Its contents cannot change either (`immut`).
    is_imut: bool,
    kind: SymbolKind,
    /// Module that declared the symbol, when it was imported: it is read-only here.
    owner: Option<String>,
    /// For functions with `mut` or `take` parameters: how each parameter is passed (empty
    /// when every parameter is borrowed).
    modes: Vec<ParamMode>,
    /// Why the variable is read-only, when the reason is not its declaration (a parameter,
    /// `self`, the `for` variable...).
    reason: Option<&'static str>,
    /// The variable borrows its value (a parameter, the `for` variable...): it cannot move it.
    /// The text says what the variable is.
    borrowed: Option<&'static str>,
    /// A copy of a variable with a narrower type (`if (x is Some)`): moving it moves the
    /// original too.
    narrowed: bool,
    /// For a generic function: the number of its type parameters (`Type::Param(0..)` in `ty`),
    /// and their constraints.
    generics: usize,
    constraints: Vec<Constraint>,
    /// The `VarDecl` that declared it, so its final type can be recorded
    /// (`Ownership::variable_types`).
    decl: Option<usize>,
    /// Line where it is declared.
    line: usize,
}

impl Symbol {
    fn value(ty: Type, is_const: bool) -> Self {
        Symbol {
            ty,
            is_const,
            is_imut: false,
            kind: SymbolKind::Value,
            owner: None,
            modes: Vec::new(),
            reason: None,
            borrowed: None,
            narrowed: false,
            generics: 0,
            constraints: Vec::new(),
            decl: None,
            line: 0,
        }
    }

    fn builtin(name: &str) -> Self {
        Symbol {
            ty: Type::Void,
            is_const: true,
            is_imut: true,
            kind: SymbolKind::Builtin(name.to_string()),
            owner: None,
            modes: Vec::new(),
            reason: None,
            borrowed: None,
            narrowed: false,
            generics: 0,
            constraints: Vec::new(),
            decl: None,
            line: 0,
        }
    }
}

#[derive(Debug, Clone)]
enum TypeRef {
    Type(Type),
    /// Generic built-in type that needs arguments (`Option`, `Result`).
    Generic(TypeId),
}

#[derive(Debug, Default, Clone)]
struct Scope {
    vars: HashMap<String, Symbol>,
    types: HashMap<String, TypeRef>,
}

#[derive(Debug, Default, Clone)]
struct ModuleType {
    /// How the module is named in messages (`math.kj`, `json`).
    display: String,
    /// Exported values, functions and constructors.
    members: HashMap<String, Symbol>,
    /// Exported types.
    types: HashMap<String, TypeRef>,
    /// Names declared in the module but not exported.
    private: HashSet<String>,
}

/// A method (`function Person.greet(self)`) or an associated function (`function
/// Person.create(name: string)`).
#[derive(Debug, Clone)]
struct MethodInfo {
    /// Full signature; for a method, the first parameter is `self`.
    ty: Type,
    has_self: bool,
    modes: Vec<ParamMode>,
    module: PathBuf,
    /// Type parameters of the type (first) and of the method itself: `Type::Param(0..)` in
    /// `ty`.
    generics: usize,
    /// The type parameters themselves, to check the method's body, and their constraints.
    type_params: Vec<TypeParam>,
    constraints: Vec<Constraint>,
}

struct ReturnContext {
    /// Declared return type; `None` when it must be inferred (anonymous functions).
    declared: Option<Type>,
    inferred: Option<Type>,
}

/// Ids of the types the language and its standard library provide.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinTypes {
    pub option: TypeId,
    pub result: TypeId,
    pub parse_error: TypeId,
    pub math_error: TypeId,
    pub file_error: TypeId,
    pub file_mode: TypeId,
    pub http_error: TypeId,
    pub json: TypeId,
    pub map: TypeId,
    pub output: TypeId,
    pub system_error: TypeId,
    pub set: TypeId,
    pub json_error: TypeId,
    pub date_error: TypeId,
    pub file: TypeId,
    pub datetime: TypeId,
    pub response: TypeId,
    pub regex_match: TypeId,
    pub regex: TypeId,
    pub regex_error: TypeId,
}

/// Per-module checking state.
#[derive(Default)]
struct ModuleState {
    /// Whether this is the main file (where `?` may be used at the top level).
    is_entry: bool,
    scopes: Vec<Scope>,
    /// Index in `scopes` of the parameter scope of each function being checked.
    function_scopes: Vec<usize>,
    returns: Vec<ReturnContext>,
    file: String,
    /// Path of the module being checked (identifies its methods at runtime).
    path: PathBuf,
    dir: PathBuf,
    line: usize,
    /// Variables whose value was moved.
    moved: Moves,
    /// Scope index where each loop being checked starts.
    loop_scopes: Vec<usize>,
    /// While checking the patterns of a `match` on a variable: its bindings borrow.
    borrowing_match: bool,
}

pub struct Checker {
    pub table: TypeTable,
    pub builtins: BuiltinTypes,
    modules: Vec<ModuleType>,
    module_by_path: HashMap<PathBuf, ModuleId>,
    stdlib: HashMap<String, ModuleId>,
    loading: Vec<PathBuf>,
    asts: HashMap<PathBuf, Rc<Vec<AstNode>>>,
    /// Methods and associated functions of each type, by type and name.
    methods: HashMap<(TypeId, String), MethodInfo>,
    /// Argument types of the last built-in call checked.
    last_arg_types: Vec<Type>,
    /// The type arguments of the last generic call checked, until the call records them.
    type_args: Option<Vec<Type>>,
    ownership: Ownership,
    state: ModuleState,
}

fn is_statement(node: &AstNode) -> bool {
    matches!(
        node.unlocated(),
        AstNode::While { .. }
            | AstNode::For { .. }
            | AstNode::Assign { .. }
            | AstNode::VarDecl { .. }
            | AstNode::FunctionDecl { .. }
            | AstNode::TypeDecl { .. }
            | AstNode::Import { .. }
    )
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    /// Checks the file at `path` and every module it imports.
    pub fn check_file(mut self, path: &Path) -> CheckResult<Program> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            EzError::ImportError(format!("cannot read '{}': {}", path.display(), e))
        })?;
        let entry = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.check_module(&entry, &path.display().to_string(), &source)?;
        Ok(Program {
            entry,
            modules: self.asts,
            ownership: self.ownership,
            table: Rc::new(self.table),
        })
    }

    /// Checks source code as if it were the file `path` (imports are relative to its directory).
    pub fn check_source(mut self, path: &Path, source: &str) -> CheckResult<Program> {
        let entry = path.to_path_buf();
        self.check_module(&entry, &path.display().to_string(), source)?;
        Ok(Program {
            entry,
            modules: self.asts,
            ownership: self.ownership,
            table: Rc::new(self.table),
        })
    }

    fn err<T>(&self, message: String) -> CheckResult<T> {
        let location = if self.state.line == 0 {
            self.state.file.clone()
        } else {
            format!("{}:{}", self.state.file, self.state.line)
        };
        Err(EzError::TypeError(format!("{}: {}", location, message)))
    }

    fn show(&self, ty: &Type) -> String {
        self.table.display(ty)
    }

    fn push_scope(&mut self) {
        self.state.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        self.state.scopes.pop();
        let len = self.state.scopes.len();
        self.state.moved.retain(|(index, _), _| *index < len);
    }

    fn current_scope(&mut self) -> &mut Scope {
        self.state.scopes.last_mut().expect("scope stack is never empty")
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.state
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
    }

    /// A function captures a read-only copy of the variables of the functions around it: they
    /// cannot be assigned (module variables are not captured, they belong to the module).
    fn ensure_not_captured(&self, name: &str) -> CheckResult<()> {
        let boundary = match self.state.function_scopes.last() {
            Some(boundary) => *boundary,
            None => return Ok(()),
        };
        let index = self
            .state
            .scopes
            .iter()
            .rposition(|scope| scope.vars.contains_key(name));
        match index {
            // 0 is the prelude and 1 the module scope.
            Some(index) if index > 1 && index < boundary => self.err(format!(
                "cannot modify '{}' here: functions capture a read-only copy of the variables around them",
                name
            )),
            _ => Ok(()),
        }
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        self.state
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.vars.get_mut(name))
    }

    fn lookup_type(&self, name: &str) -> Option<&TypeRef> {
        self.state
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.types.get(name))
    }

    /// Gives a better-known type to a variable, and records it for the compiler.
    fn set_variable_type(&mut self, name: &str, ty: Type) {
        let mut decl = None;
        if let Some(symbol) = self.lookup_mut(name) {
            symbol.ty = ty.clone();
            decl = symbol.decl;
        }
        if let Some(decl) = decl {
            self.ownership.variable_types.insert(decl, ty);
        }
    }

    fn declare(&mut self, name: &str, mut symbol: Symbol) -> CheckResult<()> {
        if self.current_scope().vars.contains_key(name) {
            return self.err(format!("'{}' is already declared in this scope", name));
        }
        symbol.line = self.state.line;
        if let Some(decl) = symbol.decl {
            self.ownership.variable_types.insert(decl, symbol.ty.clone());
        }
        self.current_scope().vars.insert(name.to_string(), symbol);
        Ok(())
    }

    /// Every variable of the current scope must have a known type by its end (`mut items =
    /// []` needs a later `items.push(1)`, or an annotation).
    fn ensure_inferred(&mut self) -> CheckResult<()> {
        let unknown = self
            .current_scope()
            .vars
            .iter()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Value) && symbol.ty.contains_unknown())
            .map(|(name, symbol)| (symbol.line, name.clone(), symbol.ty.clone(), symbol.is_imut, symbol.reason))
            .min_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        match unknown {
            // A `for` variable or a `match` binding: its type comes from another value.
            Some((line, name, ty, _, Some(_))) => {
                self.state.line = line;
                self.err(format!(
                    "the type of '{}' cannot be inferred (it is {}): give a type to the value it comes from",
                    name,
                    self.show(&ty)
                ))
            }
            Some((line, name, ty, is_imut, None)) => {
                self.state.line = line;
                // An example annotation: the type found so far, with `int` for the unknown parts.
                let example = self.show(&ty.fill_unknown(&Type::Int));
                self.err(format!(
                    "the type of '{}' cannot be inferred (it is {}): annotate it, as in '{} {}: {} = ...'",
                    name,
                    self.show(&ty),
                    if is_imut { "immut" } else { "mut" },
                    name,
                    example
                ))
            }
            None => Ok(()),
        }
    }

    /// Ends a scope of variables: they must all have known types.
    fn end_scope(&mut self) -> CheckResult<()> {
        let result = self.ensure_inferred();
        self.pop_scope();
        result
    }

    fn option_of(&self, ty: Type) -> Type {
        Type::Named {
            id: self.builtins.option,
            args: vec![ty],
        }
    }

    fn named(id: TypeId) -> Type {
        Type::Named { id, args: vec![] }
    }


}

/// Turns `line N: message` into `file:N: message`.
fn prefix_error(file: &str, error: EzError) -> EzError {
    let locate = |message: String| -> String {
        if let Some(rest) = message.strip_prefix("line ") {
            if let Some((line, text)) = rest.split_once(": ") {
                if line.chars().all(|c| c.is_ascii_digit()) {
                    return format!("{}:{}: {}", file, line, text);
                }
            }
        }
        format!("{}: {}", file, message)
    };
    match error {
        EzError::LexerError(message) => EzError::LexerError(locate(message)),
        EzError::ParserError(message) => EzError::ParserError(locate(message)),
        other => other,
    }
}

/// Checks the file at `path` (and its imports).
pub fn check_types(path: &Path) -> Result<Program, EzError> {
    Checker::new().check_file(path)
}
