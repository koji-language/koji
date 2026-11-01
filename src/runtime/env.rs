use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::runtime::value::Value;

/// A scope of variables. Blocks and function calls create child scopes.
#[derive(Debug, Default)]
pub struct Env {
    vars: RefCell<HashMap<String, Value>>,
    /// Names imported from another module: always read from that module, so the
    /// importer sees the current value of an exported `var`.
    aliases: RefCell<HashMap<String, Rc<Env>>>,
    parent: Option<Rc<Env>>,
    /// The top-level scope of a module (its variables are not captured by functions).
    is_module: bool,
    /// Variables of this scope that own a move-only value, in declaration order: their values
    /// are dropped when the scope ends.
    owned: RefCell<Vec<String>>,
    /// Variables of this scope whose value was moved out.
    moved: RefCell<HashSet<String>>,
}

impl Env {
    pub fn new(parent: Option<Rc<Env>>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            aliases: RefCell::new(HashMap::new()),
            parent,
            is_module: false,
            owned: RefCell::default(),
            moved: RefCell::default(),
        })
    }

    /// The top-level scope of a module.
    pub fn module(prelude: &Rc<Env>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            aliases: RefCell::new(HashMap::new()),
            parent: Some(Rc::clone(prelude)),
            is_module: true,
            owned: RefCell::default(),
            moved: RefCell::default(),
        })
    }

    /// Scope for a function created here: a copy of the variables of the functions and blocks
    /// around it, up to the module scope (functions capture a read-only copy).
    pub fn snapshot(env: &Rc<Env>) -> Rc<Env> {
        if env.is_module {
            return Rc::clone(env);
        }
        let mut vars: HashMap<String, Value> = HashMap::new();
        let mut aliases: HashMap<String, Rc<Env>> = HashMap::new();
        let mut current = Rc::clone(env);
        while !current.is_module {
            for (name, value) in current.vars.borrow().iter() {
                vars.entry(name.clone()).or_insert_with(|| value.clone());
            }
            for (name, module) in current.aliases.borrow().iter() {
                aliases.entry(name.clone()).or_insert_with(|| Rc::clone(module));
            }
            match &current.parent {
                Some(parent) => current = Rc::clone(parent),
                None => break,
            }
        }
        if !current.is_module {
            return Rc::clone(env);
        }
        Rc::new(Env {
            vars: RefCell::new(vars),
            aliases: RefCell::new(aliases),
            parent: Some(current),
            is_module: false,
            owned: RefCell::default(),
            moved: RefCell::default(),
        })
    }

    pub fn child(parent: &Rc<Env>) -> Rc<Env> {
        Env::new(Some(Rc::clone(parent)))
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.vars.borrow().get(name) {
            return Some(value.clone());
        }
        if let Some(module) = self.aliases.borrow().get(name) {
            return module.get_local(name);
        }
        self.parent.as_ref().and_then(|parent| parent.get(name))
    }

    /// Makes `name` refer to the variable of the same name in `module`.
    pub fn alias(&self, name: &str, module: &Rc<Env>) {
        self.aliases
            .borrow_mut()
            .insert(name.to_string(), Rc::clone(module));
    }

    /// Value defined in this scope only (used for module members).
    pub fn get_local(&self, name: &str) -> Option<Value> {
        self.vars.borrow().get(name).cloned()
    }

    pub fn define(&self, name: &str, value: Value) {
        self.moved.borrow_mut().remove(name);
        self.vars.borrow_mut().insert(name.to_string(), value);
    }

    /// Moves the value out of the variable `name`: the variable is left without a value.
    pub fn take(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.vars.borrow_mut().remove(name) {
            self.moved.borrow_mut().insert(name.to_string());
            return Some(value);
        }
        self.parent.as_ref().and_then(|parent| parent.take(name))
    }

    /// Whether `name` is a variable whose value was moved out.
    pub fn was_moved(&self, name: &str) -> bool {
        if self.vars.borrow().contains_key(name) {
            return false;
        }
        self.moved.borrow().contains(name) || self.parent.as_ref().is_some_and(|parent| parent.was_moved(name))
    }

    /// Gives a value again to a variable whose value was moved out.
    pub fn restore(&self, name: &str, value: Value) -> bool {
        if self.moved.borrow_mut().remove(name) {
            self.vars.borrow_mut().insert(name.to_string(), value);
            return true;
        }
        match &self.parent {
            Some(parent) => parent.restore(name, value),
            None => false,
        }
    }

    /// Records that the variable `name` (in the scope that declares it) owns its value.
    pub fn own(&self, name: &str) {
        if self.vars.borrow().contains_key(name) || self.moved.borrow().contains(name) {
            let mut owned = self.owned.borrow_mut();
            if !owned.iter().any(|n| n == name) {
                owned.push(name.to_string());
            }
            return;
        }
        if let Some(parent) = &self.parent {
            parent.own(name);
        }
    }

    /// Records that every variable of this scope owns its value; `false` if there is none.
    pub fn own_all(&self) -> bool {
        let names: Vec<String> = self.vars.borrow().keys().cloned().collect();
        let mut owned = self.owned.borrow_mut();
        for name in &names {
            if !owned.contains(name) {
                owned.push(name.clone());
            }
        }
        !names.is_empty()
    }

    /// Takes the values owned by this scope, last declared first, to drop them.
    pub fn take_owned(&self) -> Vec<Value> {
        let names = std::mem::take(&mut *self.owned.borrow_mut());
        let mut vars = self.vars.borrow_mut();
        names.iter().rev().filter_map(|name| vars.remove(name)).collect()
    }

    /// Runs `f` on the variable `name` itself (in this scope, an enclosing one or the module it
    /// was imported from); `None` if it does not exist.
    pub fn with_slot<R>(&self, name: &str, f: impl FnOnce(&mut Value) -> R) -> Option<R> {
        if let Some(slot) = self.vars.borrow_mut().get_mut(name) {
            return Some(f(slot));
        }
        if let Some(module) = self.aliases.borrow().get(name) {
            return module.with_slot(name, f);
        }
        match &self.parent {
            Some(parent) => parent.with_slot(name, f),
            None => None,
        }
    }

    /// Updates an existing variable in this scope or an enclosing one; `false` if not found.
    pub fn assign(&self, name: &str, value: Value) -> bool {
        if let Some(slot) = self.vars.borrow_mut().get_mut(name) {
            *slot = value;
            return true;
        }
        match &self.parent {
            Some(parent) => parent.assign(name, value),
            None => false,
        }
    }
}
