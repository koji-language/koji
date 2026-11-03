//! The text form of the AST printed by `kojic dump ast`, shared with the compiler written in
//! Koji (`compiler/parser.kj`): one S-expression per top-level statement.

use super::ast::*;

fn text(value: &str) -> String {
    koji_runtime::repr_string(value)
}

/// What `kojic dump types` adds after a node (`:int`, `!move`...); empty for `kojic dump ast`.
pub type Annotate<'a> = &'a dyn Fn(&AstNode) -> String;

fn optional(node: &Option<Box<AstNode>>, a: Annotate) -> String {
    node.as_ref().map(|node| annotated(node, a)).unwrap_or_else(|| "_".to_string())
}

fn list(nodes: &[AstNode], a: Annotate) -> String {
    nodes.iter().map(|node| format!(" {}", annotated(node, a))).collect()
}

/// A written type, as in the source: `int`, `Map[string, int]`, `int[]`, `(int) => string`.
pub fn type_text(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name) => name.clone(),
        TypeExpr::Generic(name, args) => {
            format!("{}[{}]", name, args.iter().map(type_text).collect::<Vec<_>>().join(", "))
        }
        TypeExpr::Array(element) if matches!(**element, TypeExpr::Function { .. }) => {
            format!("({})[]", type_text(element))
        }
        TypeExpr::Array(element) => format!("{}[]", type_text(element)),
        TypeExpr::Function { params, ret } => format!(
            "({}) => {}",
            params.iter().map(type_text).collect::<Vec<_>>().join(", "),
            type_text(ret)
        ),
    }
}

fn optional_type(ty: &Option<TypeExpr>) -> String {
    ty.as_ref().map(type_text).unwrap_or_else(|| "_".to_string())
}

fn params(params: &[Param]) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|param| {
            let mode = if param.is_mut { "mut " } else if param.is_take { "take " } else { "" };
            format!("(param {}{} {})", mode, param.name, optional_type(&param.ty))
        })
        .collect();
    format!("({})", params.join(" "))
}

fn fields(fields: &[Field]) -> String {
    fields.iter().map(|field| format!(" (field {} {})", field.name, type_text(&field.ty))).collect()
}

fn definition(value: &TypeDef) -> String {
    match value {
        TypeDef::Sum(variants) => {
            let variants: String = variants
                .iter()
                .map(|variant| format!(" (variant {}{})", variant.name, fields(&variant.fields)))
                .collect();
            format!("(sum{})", variants)
        }
        TypeDef::Shape(shape) => format!("(shape{})", fields(shape)),
        TypeDef::Index { key, value } => format!("(index-signature {} {})", type_text(key), type_text(value)),
        TypeDef::Intersection(parts) => {
            format!("(intersection{})", parts.iter().map(|part| format!(" {}", definition(part))).collect::<String>())
        }
        TypeDef::Alias(ty) => format!("(alias {})", type_text(ty)),
    }
}

fn pattern(pattern: &Pattern, a: Annotate) -> String {
    match pattern {
        Pattern::Wildcard => "(wildcard)".to_string(),
        Pattern::Identifier(name) => format!("(bind {})", name),
        Pattern::Variant { name, args } => {
            format!("(variant {}{})", name, args.iter().map(|arg| format!(" {}", self::pattern(arg, a))).collect::<String>())
        }
        Pattern::Literal(literal) => format!("(literal {})", annotated(literal, a)),
    }
}

fn unary(op: &UnaryOperator) -> &'static str {
    match op {
        UnaryOperator::Negate => "-",
        UnaryOperator::Not => "not",
        UnaryOperator::BitNot => "~",
    }
}

fn assign(op: &AssignOperator) -> &'static str {
    match op {
        AssignOperator::Assign => "=",
        AssignOperator::Add => "+=",
        AssignOperator::Subtract => "-=",
        AssignOperator::Multiply => "*=",
        AssignOperator::Divide => "/=",
        AssignOperator::Power => "**=",
        AssignOperator::Modulo => "%=",
    }
}

/// One node as an S-expression: `(binary + (int 1) (name x))`.
pub fn dump(node: &AstNode) -> String {
    annotated(node, &|_| String::new())
}

/// `dump`, with what `a` gives for each node after it (`Located` and `Export` are the node
/// they wrap, so only that node is annotated).
pub fn annotated(node: &AstNode, a: Annotate) -> String {
    match node {
        AstNode::Located { line, node } => format!("(at {} {})", line, annotated(node, a)),
        AstNode::Export(node) => format!("(export {})", annotated(node, a)),
        other => format!("{}{}", plain(other, a), a(other)),
    }
}

fn plain(node: &AstNode, a: Annotate) -> String {
    let dump = |node: &AstNode| annotated(node, a);
    let list = |nodes: &[AstNode]| list(nodes, a);
    let optional = |node: &Option<Box<AstNode>>| optional(node, a);
    match node {
        AstNode::IntLiteral(n) => format!("(int {})", n),
        AstNode::FloatLiteral(f) => format!("(float {})", koji_runtime::format_float(*f)),
        AstNode::StringLiteral(value) => format!("(string {})", text(value)),
        AstNode::TemplateString(parts) => {
            let parts: String = parts
                .iter()
                .map(|part| match part {
                    TemplatePart::Text(value) => format!(" (text {})", text(value)),
                    TemplatePart::Expression(expression, _) => format!(" (expr {})", dump(expression)),
                })
                .collect();
            format!("(template{})", parts)
        }
        AstNode::BooleanLiteral(b) => format!("(bool {})", b),
        AstNode::ArrayLiteral(items) => format!("(list{})", list(items)),
        AstNode::MapLiteral(entries) => {
            let entries: String = entries
                .iter()
                .map(|(key, value)| format!(" (entry {} {})", dump(key), dump(value)))
                .collect();
            format!("(map{})", entries)
        }
        AstNode::ObjectLiteral(entries) => {
            let entries: String = entries
                .iter()
                .map(|entry| format!(" (field {} {})", text(&entry.key), dump(&entry.value)))
                .collect();
            format!("(object{})", entries)
        }
        AstNode::Identifier(name) => format!("(name {})", name),
        AstNode::Unary { op, operand } => format!("(unary {} {})", unary(op), dump(operand)),
        AstNode::BinaryOp { left, op, right } => format!("(binary {} {} {})", op.symbol(), dump(left), dump(right)),
        AstNode::Call { callee, args, .. } => format!("(call {}{})", dump(callee), list(args)),
        AstNode::MutArg(place) => format!("(mut {})", dump(place)),
        AstNode::Index { target, index } => format!("(index {} {})", dump(target), dump(index)),
        AstNode::Slice { target, start, end } => {
            format!("(slice {} {} {})", dump(target), optional(start), optional(end))
        }
        AstNode::Member { object, name } => format!("(member {} {})", dump(object), name),
        AstNode::If { condition, then_branch, else_branch } => {
            format!("(if {} {} {})", dump(condition), dump(then_branch), optional(else_branch))
        }
        AstNode::Match { scrutinee, arms } => {
            let arms: String = arms
                .iter()
                .map(|arm| {
                    let guard = arm.guard.as_ref().map(dump).unwrap_or_else(|| "_".to_string());
                    format!(" (arm {} {} {})", pattern(&arm.pattern, a), guard, dump(&arm.body))
                })
                .collect();
            format!("(match {}{})", dump(scrutinee), arms)
        }
        AstNode::Lambda { params: lambda_params, return_type, body } => {
            format!("(lambda {} {} {})", params(lambda_params), optional_type(return_type), dump(body))
        }
        AstNode::Block(statements) => format!("(block{})", list(statements)),
        AstNode::While { condition, body } => format!("(while {} {})", dump(condition), dump(body)),
        AstNode::For { variable, is_mut, iterable, body } => {
            let mode = if *is_mut { "mut " } else { "" };
            format!("(for {}{} {} {})", mode, variable, dump(iterable), dump(body))
        }
        AstNode::Break => "(break)".to_string(),
        AstNode::Continue => "(continue)".to_string(),
        AstNode::Return(None) => "(return)".to_string(),
        AstNode::Return(Some(value)) => format!("(return {})", dump(value)),
        AstNode::Assign { target, op, value } => format!("(assign {} {} {})", assign(op), dump(target), dump(value)),
        AstNode::VarDecl { name, ty, value, is_imut, .. } => {
            let keyword = if *is_imut { "immut" } else { "mut" };
            format!("({} {} {} {})", keyword, name, optional_type(ty), dump(value))
        }
        AstNode::FunctionDecl { name, owner, type_params, params: function_params, return_type, body } => {
            let name = match owner {
                Some(owner) => format!("{}.{}", owner, name),
                None => name.clone(),
            };
            format!(
                "(function {} {} {} {} {})",
                name,
                type_parameters(type_params),
                params(function_params),
                optional_type(return_type),
                dump(body)
            )
        }
        AstNode::TypeDecl { name, params: type_params, definition: def } => {
            format!("(type {} {} {})", name, type_parameters(type_params), definition(def))
        }
        AstNode::Import { kind: ImportKind::Module(name), path } => format!("(import {} {})", name, text(path)),
        AstNode::Import { kind: ImportKind::Names(names), path } => {
            format!("(import ({}) {})", names.join(" "), text(path))
        }
        AstNode::Try { value, .. } => format!("(try {})", dump(value)),
        AstNode::Export(node) => format!("(export {})", dump(node)),
        AstNode::Located { line, node } => format!("(at {} {})", line, dump(node)),
    }
}

fn type_parameters(params: &[TypeParam]) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|param| match &param.constraint {
            Some(constraint) => format!("{}:{}", param.name, constraint),
            None => param.name.clone(),
        })
        .collect();
    format!("({})", params.join(" "))
}
